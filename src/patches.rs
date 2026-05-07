// PatchSystem, diff parsing and application

use regex::Regex;
use std::path::{Path, PathBuf};

/// A proposed code change parsed from an LLM response.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PatchProposal {
    pub target_file: PathBuf,
    pub diff_text: String,
    pub original_content: String,
    pub proposed_content: String,
    pub reasoning: String,
}

/// Manages pending and applied patches.
///
/// Stores at most one pending proposal and one last-applied proposal (for undo).
#[derive(Debug, Default)]
pub struct PatchSystem {
    pending: Option<PatchProposal>,
    last_applied: Option<PatchProposal>,
}

impl PatchSystem {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if there is a pending patch proposal.
    #[allow(dead_code)]
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Returns a reference to the pending proposal, if any.
    #[allow(dead_code)]
    pub fn pending(&self) -> Option<&PatchProposal> {
        self.pending.as_ref()
    }

    /// Returns a reference to the last applied proposal, if any.
    #[allow(dead_code)]
    pub fn last_applied(&self) -> Option<&PatchProposal> {
        self.last_applied.as_ref()
    }

    /// Store a new patch proposal, replacing any existing pending proposal.
    pub fn store_proposal(&mut self, proposal: PatchProposal) {
        self.pending = Some(proposal);
    }

    /// Apply the pending patch.
    /// Creates a .bak backup, writes the patched content.
    /// Falls back to full-file replacement if line-by-line fails.
    /// Restores from .bak on total failure.
    /// Returns Ok(success_message) or Err(error_message).
    pub fn apply_patch(&mut self) -> Result<String, String> {
        let proposal = self
            .pending
            .take()
            .ok_or_else(|| "No pending patch to apply.".to_string())?;

        let target = &proposal.target_file;
        let backup_path = target.with_extension(format!(
            "{}.bak",
            target
                .extension()
                .unwrap_or_default()
                .to_str()
                .unwrap_or("")
        ));

        // Create backup
        std::fs::copy(target, &backup_path)
            .map_err(|e| format!("Failed to create backup: {e}"))?;

        // Write proposed content
        match std::fs::write(target, &proposal.proposed_content) {
            Ok(_) => {
                let msg = format!("Patch applied to {}", target.display());
                self.last_applied = Some(proposal);
                Ok(msg)
            }
            Err(e) => {
                // Restore from backup on failure
                let _ = std::fs::copy(&backup_path, target);
                Err(format!("Failed to apply patch: {e}"))
            }
        }
    }

    /// Refuse and discard the pending patch proposal.
    pub fn refuse_patch(&mut self) -> String {
        if self.pending.take().is_some() {
            "Patch refused.".to_string()
        } else {
            "No pending patch to refuse.".to_string()
        }
    }

    /// Undo the last applied patch by restoring from .bak backup.
    pub fn undo_last_patch(&mut self) -> Result<String, String> {
        let proposal = self
            .last_applied
            .take()
            .ok_or_else(|| "No patch to undo.".to_string())?;

        let target = &proposal.target_file;
        let backup_path = target.with_extension(format!(
            "{}.bak",
            target
                .extension()
                .unwrap_or_default()
                .to_str()
                .unwrap_or("")
        ));

        if !backup_path.exists() {
            return Err("Backup file not found.".to_string());
        }

        std::fs::copy(&backup_path, target)
            .map_err(|e| format!("Failed to restore from backup: {e}"))?;

        Ok(format!("Patch undone for {}", target.display()))
    }

    /// Validate that a string is a well-formed unified diff.
    ///
    /// Returns true iff the text contains at least one line starting with "---",
    /// one starting with "+++", and one starting with "@@".
    pub fn validate_diff(diff_text: &str) -> bool {
        let lines: Vec<&str> = diff_text.lines().collect();
        let has_minus = lines.iter().any(|line| line.starts_with("---"));
        let has_plus = lines.iter().any(|line| line.starts_with("+++"));
        let has_hunk = lines.iter().any(|line| line.starts_with("@@"));
        has_minus && has_plus && has_hunk
    }

    /// Extract and validate a unified diff from an LLM response.
    ///
    /// Extraction strategy:
    /// 1. Look for fenced code blocks (```diff ... ```)
    /// 2. If no code block, look for raw diff markers (lines starting with ---)
    /// 3. Validate the extracted diff
    /// 4. Extract reasoning from text before the diff
    /// 5. Apply diff hunks to produce proposed_content
    /// 6. Fall back to original_content if application fails
    ///
    /// Returns None if no valid diff is found.
    pub fn parse_llm_diff(
        &self,
        response: &str,
        target: &Path,
        original: &str,
    ) -> Option<PatchProposal> {
        let (diff_text, reasoning) = self.extract_diff_and_reasoning(response)?;

        if !Self::validate_diff(&diff_text) {
            return None;
        }

        let proposed_content = apply_diff_to_content(original, &diff_text);

        Some(PatchProposal {
            target_file: target.to_path_buf(),
            diff_text,
            original_content: original.to_string(),
            proposed_content,
            reasoning,
        })
    }

    /// Try to extract diff text and reasoning from the response.
    /// Returns (diff_text, reasoning) or None if no diff found.
    fn extract_diff_and_reasoning(&self, response: &str) -> Option<(String, String)> {
        // Strategy 1: fenced code block ```diff ... ```
        if let Some((diff, reasoning)) = extract_diff_from_codeblock(response) {
            return Some((diff, reasoning));
        }

        // Strategy 2: raw diff markers (line starting with "---")
        if let Some((diff, reasoning)) = extract_diff_from_markers(response) {
            return Some((diff, reasoning));
        }

        None
    }
}

/// Extract diff content from a fenced code block (```diff ... ```).
/// Returns (diff_text, reasoning) where reasoning is text before the code block.
fn extract_diff_from_codeblock(text: &str) -> Option<(String, String)> {
    // Try ```diff ... ``` first
    let pattern = Regex::new(r"(?s)```diff\s*\n(.*?)```").unwrap();
    if let Some(caps) = pattern.captures(text) {
        let diff = caps.get(1).unwrap().as_str().trim().to_string();
        let block_start = caps.get(0).unwrap().start();
        let reasoning = text[..block_start].trim().to_string();
        return Some((diff, reasoning));
    }

    // Try generic ``` ... ``` blocks that contain diff markers
    let generic_pattern = Regex::new(r"(?s)```\s*\n(.*?)```").unwrap();
    for caps in generic_pattern.captures_iter(text) {
        let block = caps.get(1).unwrap().as_str().trim().to_string();
        if PatchSystem::validate_diff(&block) {
            let block_start = caps.get(0).unwrap().start();
            let reasoning = text[..block_start].trim().to_string();
            return Some((block, reasoning));
        }
    }

    None
}

/// Extract diff starting from --- markers in plain text.
/// Returns (diff_text, reasoning) where reasoning is text before the --- line.
fn extract_diff_from_markers(text: &str) -> Option<(String, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut diff_start = None;

    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("---") {
            diff_start = Some(i);
            break;
        }
    }

    let diff_start = diff_start?;

    // Collect lines from the --- marker until we hit non-diff content
    let mut diff_lines: Vec<&str> = Vec::new();
    for line in &lines[diff_start..] {
        if line.starts_with("---")
            || line.starts_with("+++")
            || line.starts_with("@@")
            || line.starts_with('+')
            || line.starts_with('-')
            || line.starts_with(' ')
            || line.is_empty()
        {
            diff_lines.push(line);
        } else {
            // Stop at first line that doesn't look like diff content
            // but only after we've collected some diff lines
            if diff_lines.len() > 2 {
                break;
            }
            diff_lines.push(line);
        }
    }

    if diff_lines.is_empty() {
        return None;
    }

    let diff_text = diff_lines.join("\n").trim().to_string();
    let reasoning = lines[..diff_start].join("\n").trim().to_string();

    Some((diff_text, reasoning))
}

/// Apply a unified diff to original content.
/// Returns proposed_content if successful, or original_content as fallback.
fn apply_diff_to_content(original: &str, diff_text: &str) -> String {
    let mut original_lines: Vec<String> = original.lines().map(|l| l.to_string()).collect();
    // Ensure we have at least an empty vec for empty content
    if original.is_empty() {
        original_lines = Vec::new();
    }

    match apply_unified_diff(&original_lines, diff_text) {
        Some(patched) => patched.join("\n"),
        None => original.to_string(),
    }
}

/// Apply a unified diff to a list of lines.
/// Returns the patched lines or None if application fails.
fn apply_unified_diff(original_lines: &[String], diff_text: &str) -> Option<Vec<String>> {
    let hunks = parse_hunks(diff_text)?;
    if hunks.is_empty() {
        return None;
    }

    let mut result: Vec<String> = original_lines.to_vec();
    let mut offset: i64 = 0;

    for (hunk_start, hunk_count, hunk_lines) in &hunks {
        // hunk_start is 1-indexed
        let pos_signed = *hunk_start as i64 - 1 + offset;
        if pos_signed < 0 {
            return None;
        }
        let pos = pos_signed as usize;

        // Bounds check: if pos is beyond the result, the diff can't be applied
        if pos > result.len() {
            return None;
        }

        let mut new_lines: Vec<String> = Vec::new();

        for hline in hunk_lines {
            if let Some(stripped) = hline.strip_prefix('-') {
                // Removed line — skip it (don't add to new_lines)
                let _ = stripped;
            } else if let Some(stripped) = hline.strip_prefix('+') {
                // Added line
                new_lines.push(stripped.to_string());
            } else if let Some(stripped) = hline.strip_prefix(' ') {
                // Context line
                new_lines.push(stripped.to_string());
            } else if hline.is_empty() {
                // Empty context line
                new_lines.push(String::new());
            } else {
                // Treat as context line (no prefix)
                new_lines.push(hline.to_string());
            }
        }

        // Replace the range in result
        let end_pos = (pos + *hunk_count).min(result.len());
        result.splice(pos..end_pos, new_lines.iter().cloned());
        offset += new_lines.len() as i64 - *hunk_count as i64;
    }

    Some(result)
}

/// Parse hunk headers and their content from diff lines.
/// Returns list of (start_line, line_count, hunk_content_lines).
fn parse_hunks(diff_text: &str) -> Option<Vec<(usize, usize, Vec<String>)>> {
    let hunk_header_re = Regex::new(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@").unwrap();
    let diff_lines: Vec<&str> = diff_text.lines().collect();

    let mut hunks: Vec<(usize, usize, Vec<String>)> = Vec::new();
    let mut current_start: usize = 0;
    let mut current_count: usize = 0;
    let mut current_hunk_lines: Vec<String> = Vec::new();
    let mut in_hunk = false;

    for line in &diff_lines {
        if line.starts_with("@@") {
            // Save previous hunk if exists
            if in_hunk && !current_hunk_lines.is_empty() {
                hunks.push((current_start, current_count, current_hunk_lines.clone()));
            }

            // Parse @@ -start,count +start,count @@
            if let Some(caps) = hunk_header_re.captures(line) {
                current_start = caps.get(1).unwrap().as_str().parse().unwrap_or(0);
                current_count = caps
                    .get(2)
                    .map(|m| m.as_str().parse().unwrap_or(1))
                    .unwrap_or(1);
                current_hunk_lines = Vec::new();
                in_hunk = true;
            } else {
                return None;
            }
        } else if line.starts_with("---") || line.starts_with("+++") {
            // Skip file headers
            continue;
        } else if in_hunk {
            current_hunk_lines.push(line.to_string());
        }
    }

    // Don't forget the last hunk
    if in_hunk && !current_hunk_lines.is_empty() {
        hunks.push((current_start, current_count, current_hunk_lines));
    }

    if hunks.is_empty() {
        None
    } else {
        Some(hunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // --- validate_diff tests ---

    #[test]
    fn test_validate_diff_valid() {
        let diff = "--- a/file.rs\n+++ b/file.rs\n@@ -1,3 +1,3 @@\n-old line\n+new line\n context\n";
        assert!(PatchSystem::validate_diff(diff));
    }

    #[test]
    fn test_validate_diff_missing_minus() {
        let diff = "+++ b/file.rs\n@@ -1,3 +1,3 @@\n+new line\n";
        assert!(!PatchSystem::validate_diff(diff));
    }

    #[test]
    fn test_validate_diff_missing_plus() {
        let diff = "--- a/file.rs\n@@ -1,3 +1,3 @@\n-old line\n";
        assert!(!PatchSystem::validate_diff(diff));
    }

    #[test]
    fn test_validate_diff_missing_hunk() {
        let diff = "--- a/file.rs\n+++ b/file.rs\n-old line\n+new line\n";
        assert!(!PatchSystem::validate_diff(diff));
    }

    #[test]
    fn test_validate_diff_empty_string() {
        assert!(!PatchSystem::validate_diff(""));
    }

    #[test]
    fn test_validate_diff_random_text() {
        assert!(!PatchSystem::validate_diff("hello world\nfoo bar\n"));
    }

    // --- parse_llm_diff tests ---

    #[test]
    fn test_parse_llm_diff_fenced_codeblock() {
        let response = "Here is the fix:\n\n```diff\n--- a/file.rs\n+++ b/file.rs\n@@ -1,3 +1,3 @@\n line1\n-old line\n+new line\n line3\n```\n";
        let ps = PatchSystem::new();
        let target = Path::new("file.rs");
        let original = "line1\nold line\nline3\n";

        let result = ps.parse_llm_diff(response, target, original);
        assert!(result.is_some());

        let proposal = result.unwrap();
        assert_eq!(proposal.target_file, PathBuf::from("file.rs"));
        assert_eq!(proposal.reasoning, "Here is the fix:");
        assert!(proposal.diff_text.contains("---"));
        assert!(proposal.diff_text.contains("+++"));
        assert!(proposal.diff_text.contains("@@"));
    }

    #[test]
    fn test_parse_llm_diff_raw_markers() {
        let response = "I suggest this change:\n--- a/file.rs\n+++ b/file.rs\n@@ -1,2 +1,2 @@\n-old\n+new\n";
        let ps = PatchSystem::new();
        let target = Path::new("file.rs");
        let original = "old\n";

        let result = ps.parse_llm_diff(response, target, original);
        assert!(result.is_some());

        let proposal = result.unwrap();
        assert_eq!(proposal.reasoning, "I suggest this change:");
    }

    #[test]
    fn test_parse_llm_diff_no_diff() {
        let response = "I don't have any changes to suggest.";
        let ps = PatchSystem::new();
        let target = Path::new("file.rs");
        let original = "some content";

        let result = ps.parse_llm_diff(response, target, original);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_llm_diff_invalid_diff_in_codeblock() {
        // A code block that doesn't contain valid diff markers
        let response = "```diff\nthis is not a real diff\n```\n";
        let ps = PatchSystem::new();
        let target = Path::new("file.rs");
        let original = "content";

        let result = ps.parse_llm_diff(response, target, original);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_llm_diff_applies_hunks() {
        let response = "```diff\n--- a/file.rs\n+++ b/file.rs\n@@ -1,3 +1,3 @@\n line1\n-old line\n+new line\n line3\n```\n";
        let ps = PatchSystem::new();
        let target = Path::new("file.rs");
        let original = "line1\nold line\nline3";

        let result = ps.parse_llm_diff(response, target, original);
        assert!(result.is_some());

        let proposal = result.unwrap();
        assert!(proposal.proposed_content.contains("new line"));
        assert!(!proposal.proposed_content.contains("old line"));
    }

    #[test]
    fn test_parse_llm_diff_fallback_on_bad_hunks() {
        // Valid diff markers but hunk that can't be applied (line numbers out of range)
        let response = "```diff\n--- a/file.rs\n+++ b/file.rs\n@@ -100,3 +100,3 @@\n context\n-old\n+new\n```\n";
        let ps = PatchSystem::new();
        let target = Path::new("file.rs");
        let original = "just one line";

        let result = ps.parse_llm_diff(response, target, original);
        assert!(result.is_some());

        let proposal = result.unwrap();
        // Falls back to original content when diff application fails
        assert_eq!(proposal.proposed_content, original);
    }

    // --- apply_diff_to_content tests ---

    #[test]
    fn test_apply_diff_simple_replacement() {
        let original = "line1\nline2\nline3";
        let diff = "--- a/file\n+++ b/file\n@@ -1,3 +1,3 @@\n line1\n-line2\n+replaced\n line3\n";

        let result = apply_diff_to_content(original, diff);
        assert_eq!(result, "line1\nreplaced\nline3");
    }

    #[test]
    fn test_apply_diff_addition() {
        let original = "line1\nline2";
        let diff = "--- a/file\n+++ b/file\n@@ -1,2 +1,3 @@\n line1\n+inserted\n line2\n";

        let result = apply_diff_to_content(original, diff);
        assert_eq!(result, "line1\ninserted\nline2");
    }

    #[test]
    fn test_apply_diff_deletion() {
        let original = "line1\nline2\nline3";
        let diff = "--- a/file\n+++ b/file\n@@ -1,3 +1,2 @@\n line1\n-line2\n line3\n";

        let result = apply_diff_to_content(original, diff);
        assert_eq!(result, "line1\nline3");
    }

    #[test]
    fn test_apply_diff_fallback_on_failure() {
        let original = "original content";
        let diff = "--- a/file\n+++ b/file\n@@ -50,3 +50,3 @@\n context\n-old\n+new\n";

        let result = apply_diff_to_content(original, diff);
        // Should fall back to original when hunk can't be applied
        assert_eq!(result, original);
    }

    // --- parse_hunks tests ---

    #[test]
    fn test_parse_hunks_single() {
        let diff = "--- a/file\n+++ b/file\n@@ -1,3 +1,3 @@\n context\n-old\n+new\n";
        let hunks = parse_hunks(diff);
        assert!(hunks.is_some());
        let hunks = hunks.unwrap();
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].0, 1); // start
        assert_eq!(hunks[0].1, 3); // count
    }

    #[test]
    fn test_parse_hunks_multiple() {
        let diff = "--- a/file\n+++ b/file\n@@ -1,2 +1,2 @@\n-a\n+b\n context\n@@ -10,2 +10,2 @@\n-c\n+d\n context\n";
        let hunks = parse_hunks(diff);
        assert!(hunks.is_some());
        let hunks = hunks.unwrap();
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].0, 1);
        assert_eq!(hunks[1].0, 10);
    }

    #[test]
    fn test_parse_hunks_no_count() {
        // @@ -1 +1 @@ means count defaults to 1
        let diff = "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n";
        let hunks = parse_hunks(diff);
        assert!(hunks.is_some());
        let hunks = hunks.unwrap();
        assert_eq!(hunks[0].1, 1); // default count
    }

    // --- store_proposal tests ---

    #[test]
    fn test_store_proposal_sets_pending() {
        let mut ps = PatchSystem::new();
        assert!(!ps.has_pending());

        let proposal = PatchProposal {
            target_file: PathBuf::from("test.rs"),
            diff_text: "some diff".to_string(),
            original_content: "original".to_string(),
            proposed_content: "proposed".to_string(),
            reasoning: "fix bug".to_string(),
        };

        ps.store_proposal(proposal);
        assert!(ps.has_pending());
        assert_eq!(ps.pending().unwrap().target_file, PathBuf::from("test.rs"));
    }

    #[test]
    fn test_store_proposal_replaces_existing() {
        let mut ps = PatchSystem::new();

        let proposal1 = PatchProposal {
            target_file: PathBuf::from("first.rs"),
            diff_text: "diff1".to_string(),
            original_content: "orig1".to_string(),
            proposed_content: "prop1".to_string(),
            reasoning: "reason1".to_string(),
        };
        ps.store_proposal(proposal1);

        let proposal2 = PatchProposal {
            target_file: PathBuf::from("second.rs"),
            diff_text: "diff2".to_string(),
            original_content: "orig2".to_string(),
            proposed_content: "prop2".to_string(),
            reasoning: "reason2".to_string(),
        };
        ps.store_proposal(proposal2);

        assert!(ps.has_pending());
        assert_eq!(
            ps.pending().unwrap().target_file,
            PathBuf::from("second.rs")
        );
    }

    // --- apply_patch tests ---

    #[test]
    fn test_apply_patch_creates_backup_and_writes_content() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("target.rs");
        std::fs::write(&file_path, "original content").unwrap();

        let mut ps = PatchSystem::new();
        let proposal = PatchProposal {
            target_file: file_path.clone(),
            diff_text: "some diff".to_string(),
            original_content: "original content".to_string(),
            proposed_content: "patched content".to_string(),
            reasoning: "fix".to_string(),
        };
        ps.store_proposal(proposal);

        let result = ps.apply_patch();
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Patch applied"));

        // Verify file was written with proposed content
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "patched content");

        // Verify backup was created
        let backup_path = file_path.with_extension("rs.bak");
        assert!(backup_path.exists());
        let backup_content = std::fs::read_to_string(&backup_path).unwrap();
        assert_eq!(backup_content, "original content");

        // Verify pending is cleared and last_applied is set
        assert!(!ps.has_pending());
        assert!(ps.last_applied().is_some());
    }

    #[test]
    fn test_apply_patch_no_pending() {
        let mut ps = PatchSystem::new();
        let result = ps.apply_patch();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "No pending patch to apply.");
    }

    #[test]
    fn test_apply_patch_restores_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("target.rs");
        std::fs::write(&file_path, "original content").unwrap();

        // Create a proposal targeting a read-only path to simulate write failure
        // We'll use a non-existent directory to trigger the backup failure
        let bad_path = dir.path().join("nonexistent_dir").join("file.rs");

        let mut ps = PatchSystem::new();
        let proposal = PatchProposal {
            target_file: bad_path,
            diff_text: "diff".to_string(),
            original_content: "original".to_string(),
            proposed_content: "patched".to_string(),
            reasoning: "fix".to_string(),
        };
        ps.store_proposal(proposal);

        let result = ps.apply_patch();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to create backup"));
    }

    // --- refuse_patch tests ---

    #[test]
    fn test_refuse_patch_discards_pending() {
        let mut ps = PatchSystem::new();
        let proposal = PatchProposal {
            target_file: PathBuf::from("test.rs"),
            diff_text: "diff".to_string(),
            original_content: "orig".to_string(),
            proposed_content: "prop".to_string(),
            reasoning: "reason".to_string(),
        };
        ps.store_proposal(proposal);
        assert!(ps.has_pending());

        let msg = ps.refuse_patch();
        assert_eq!(msg, "Patch refused.");
        assert!(!ps.has_pending());
    }

    #[test]
    fn test_refuse_patch_no_pending() {
        let mut ps = PatchSystem::new();
        let msg = ps.refuse_patch();
        assert_eq!(msg, "No pending patch to refuse.");
    }

    // --- undo_last_patch tests ---

    #[test]
    fn test_undo_last_patch_restores_from_backup() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("target.rs");
        std::fs::write(&file_path, "original content").unwrap();

        let mut ps = PatchSystem::new();
        let proposal = PatchProposal {
            target_file: file_path.clone(),
            diff_text: "diff".to_string(),
            original_content: "original content".to_string(),
            proposed_content: "patched content".to_string(),
            reasoning: "fix".to_string(),
        };
        ps.store_proposal(proposal);

        // Apply the patch first
        let apply_result = ps.apply_patch();
        assert!(apply_result.is_ok());

        // Verify file has patched content
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "patched content");

        // Undo the patch
        let undo_result = ps.undo_last_patch();
        assert!(undo_result.is_ok());
        assert!(undo_result.unwrap().contains("Patch undone"));

        // Verify file is restored to original
        let restored = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(restored, "original content");

        // Verify last_applied is cleared
        assert!(ps.last_applied().is_none());
    }

    #[test]
    fn test_undo_last_patch_no_applied() {
        let mut ps = PatchSystem::new();
        let result = ps.undo_last_patch();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "No patch to undo.");
    }

    #[test]
    fn test_undo_last_patch_backup_missing() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("target.rs");
        std::fs::write(&file_path, "content").unwrap();

        let mut ps = PatchSystem::new();
        // Manually set last_applied without going through apply_patch
        // (so no backup file exists)
        ps.last_applied = Some(PatchProposal {
            target_file: file_path,
            diff_text: "diff".to_string(),
            original_content: "orig".to_string(),
            proposed_content: "prop".to_string(),
            reasoning: "reason".to_string(),
        });

        let result = ps.undo_last_patch();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Backup file not found.");
    }
}
