use regex::Regex;

#[derive(Debug, PartialEq)]
pub enum TaskKind {
    Review,
    FixError {
        line_number: u32,
        error_text: String,
    },
    TranslationCheck,
    HeaderDateTime,
    Search {
        term: String,
    },
    RunCommand {
        command: String,
    },
    RunTest,
    GitDiff,
    FileNavigate {
        path: String,
    },
    FileCreate {
        filename: String,
        description: String,
    },
    FileEdit {
        path: String,
        instruction: String,
    },
    FileFind {
        pattern: String,
    },
    GeneralChat {
        input: String,
    },
}

/// Classify user input into a task kind.
///
/// Rules (checked in order):
/// 1. "/search <term>" → Search
/// 2. "/run <command>" → RunCommand
/// 3. "/test" → RunTest
/// 4. "/gitdiff" → GitDiff
/// 5. "/open <path>", "/cd <path>", "/goto <path>" → FileNavigate
/// 6. "/create <filename> [description]", "/new <filename> [description]" → FileCreate
/// 7. "/edit <path> <instruction>", "/insert <path> <instruction>", "/modify <path> <instruction>" → FileEdit
/// 8. "/find <pattern>", "/ls <pattern>" → FileFind
/// 9. "fix line <N>" or contains "traceback" → FixError
/// 10. Contains "review" → Review
/// 11. Contains "translations" AND file_ext is Some("html") → TranslationCheck
/// 12. Contains "add date" or "add time" or "add header" → HeaderDateTime
/// 13. Everything else → GeneralChat
pub fn classify(input: &str, file_ext: Option<&str>) -> TaskKind {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();

    // /search <term>
    if lower.starts_with("/search ") {
        let term = trimmed[8..].trim().to_string();
        return TaskKind::Search { term };
    }

    // /run <command>
    if lower.starts_with("/run ") {
        let command = trimmed[5..].trim().to_string();
        return TaskKind::RunCommand { command };
    }

    // /test
    if lower == "/test" {
        return TaskKind::RunTest;
    }

    // /gitdiff
    if lower == "/gitdiff" {
        return TaskKind::GitDiff;
    }

    // /open <path>, /cd <path>, /goto <path> → FileNavigate
    if lower.starts_with("/open ") || lower.starts_with("/cd ") || lower.starts_with("/goto ") {
        let space_idx = trimmed.find(' ').unwrap();
        let path = trimmed[space_idx + 1..].trim().to_string();
        return TaskKind::FileNavigate { path };
    }

    // /create <filename> [description], /new <filename> [description] → FileCreate
    if lower.starts_with("/create ") || lower.starts_with("/new ") {
        let space_idx = trimmed.find(' ').unwrap();
        let rest = trimmed[space_idx + 1..].trim();
        let (filename, description) = match rest.find(' ') {
            Some(idx) => (rest[..idx].to_string(), rest[idx + 1..].trim().to_string()),
            None => (rest.to_string(), String::new()),
        };
        return TaskKind::FileCreate {
            filename,
            description,
        };
    }

    // /edit <path> <instruction>, /insert <path> <instruction>, /modify <path> <instruction> → FileEdit
    if lower.starts_with("/edit ") || lower.starts_with("/insert ") || lower.starts_with("/modify ")
    {
        let space_idx = trimmed.find(' ').unwrap();
        let rest = trimmed[space_idx + 1..].trim();
        let (path, instruction) = match rest.find(' ') {
            Some(idx) => (rest[..idx].to_string(), rest[idx + 1..].trim().to_string()),
            None => (rest.to_string(), String::new()),
        };
        return TaskKind::FileEdit { path, instruction };
    }

    // /find <pattern>, /ls <pattern> → FileFind
    if lower.starts_with("/find ") || lower.starts_with("/ls ") {
        let space_idx = trimmed.find(' ').unwrap();
        let pattern = trimmed[space_idx + 1..].trim().to_string();
        return TaskKind::FileFind { pattern };
    }

    // fix line <N> or traceback
    if lower.contains("traceback") {
        return TaskKind::FixError {
            line_number: 0,
            error_text: trimmed.to_string(),
        };
    }
    let fix_re = Regex::new(r"(?i)fix\s+line\s+(\d+)").unwrap();
    if let Some(caps) = fix_re.captures(trimmed) {
        let line_number: u32 = caps[1].parse().unwrap_or(0);
        let error_text = trimmed.to_string();
        return TaskKind::FixError {
            line_number,
            error_text,
        };
    }

    // review
    if lower.contains("review") {
        return TaskKind::Review;
    }

    // translations (only for .html files)
    if lower.contains("translations") && file_ext == Some("html") {
        return TaskKind::TranslationCheck;
    }

    // add date / add time / add header
    if lower.contains("add date") || lower.contains("add time") || lower.contains("add header") {
        return TaskKind::HeaderDateTime;
    }

    // Natural language file operations (after all keyword checks, before GeneralChat fallback)

    // NL navigate: "open X", "go to X", "navigate to X", "cd X"
    let nl_navigate_re = Regex::new(r"(?i)(open|go\s+to|navigate\s+to|cd)\s+(.+)").unwrap();
    if let Some(caps) = nl_navigate_re.captures(trimmed) {
        let path = caps[2].trim().to_string();
        return TaskKind::FileNavigate { path };
    }

    // NL create: "create X", "make X", "generate X", "write X" (with optional "a" article)
    let nl_create_re = Regex::new(r"(?i)(create|make|generate|write)\s+(?:a\s+)?(.+)").unwrap();
    if let Some(caps) = nl_create_re.captures(trimmed) {
        let rest = caps[2].trim();
        let (filename, description) = match rest.find(' ') {
            Some(idx) => (rest[..idx].to_string(), rest[idx + 1..].trim().to_string()),
            None => (rest.to_string(), String::new()),
        };
        return TaskKind::FileCreate {
            filename,
            description,
        };
    }

    // NL edit: "edit X", "change X", "add to X", "update X", "insert into X"
    let nl_edit_re = Regex::new(r"(?i)(edit|change|add\s+to|update|insert\s+into)\s+(.+)").unwrap();
    if let Some(caps) = nl_edit_re.captures(trimmed) {
        let rest = caps[2].trim();
        let (path, instruction) = match rest.find(' ') {
            Some(idx) => (rest[..idx].to_string(), rest[idx + 1..].trim().to_string()),
            None => (rest.to_string(), String::new()),
        };
        return TaskKind::FileEdit { path, instruction };
    }

    // NL find: "find X", "locate X", "list files X", "where is X"
    let nl_find_re = Regex::new(r"(?i)(find|locate|list\s+files|where\s+is)\s+(.+)").unwrap();
    if let Some(caps) = nl_find_re.captures(trimmed) {
        let pattern = caps[2].trim().to_string();
        return TaskKind::FileFind { pattern };
    }

    // Default: general chat
    TaskKind::GeneralChat {
        input: trimmed.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Requirement 13.5: /search <term>
    #[test]
    fn test_classify_search() {
        let result = classify("/search foo bar", None);
        assert_eq!(
            result,
            TaskKind::Search {
                term: "foo bar".to_string()
            }
        );
    }

    #[test]
    fn test_classify_search_case_insensitive() {
        let result = classify("/Search MyTerm", None);
        assert_eq!(
            result,
            TaskKind::Search {
                term: "MyTerm".to_string()
            }
        );
    }

    #[test]
    fn test_classify_search_with_leading_whitespace() {
        let result = classify("  /search utils", None);
        assert_eq!(
            result,
            TaskKind::Search {
                term: "utils".to_string()
            }
        );
    }

    // Requirement 13.6: /run <command>
    #[test]
    fn test_classify_run_command() {
        let result = classify("/run cargo build", None);
        assert_eq!(
            result,
            TaskKind::RunCommand {
                command: "cargo build".to_string()
            }
        );
    }

    #[test]
    fn test_classify_run_command_case_insensitive() {
        let result = classify("/Run npm test", None);
        assert_eq!(
            result,
            TaskKind::RunCommand {
                command: "npm test".to_string()
            }
        );
    }

    // Requirement 13.7: /test
    #[test]
    fn test_classify_test() {
        let result = classify("/test", None);
        assert_eq!(result, TaskKind::RunTest);
    }

    #[test]
    fn test_classify_test_case_insensitive() {
        let result = classify("/Test", None);
        assert_eq!(result, TaskKind::RunTest);
    }

    #[test]
    fn test_classify_test_with_whitespace() {
        let result = classify("  /test  ", None);
        assert_eq!(result, TaskKind::RunTest);
    }

    // Requirement 13.8: /gitdiff
    #[test]
    fn test_classify_gitdiff() {
        let result = classify("/gitdiff", None);
        assert_eq!(result, TaskKind::GitDiff);
    }

    #[test]
    fn test_classify_gitdiff_case_insensitive() {
        let result = classify("/GitDiff", None);
        assert_eq!(result, TaskKind::GitDiff);
    }

    // Requirement 13.2: fix line <N> or traceback
    #[test]
    fn test_classify_fix_line() {
        let result = classify("fix line 42", None);
        assert_eq!(
            result,
            TaskKind::FixError {
                line_number: 42,
                error_text: "fix line 42".to_string(),
            }
        );
    }

    #[test]
    fn test_classify_fix_line_with_context() {
        let result = classify("please fix line 10 it has a bug", None);
        assert_eq!(
            result,
            TaskKind::FixError {
                line_number: 10,
                error_text: "please fix line 10 it has a bug".to_string(),
            }
        );
    }

    #[test]
    fn test_classify_traceback() {
        let result = classify("I got a traceback in my code", None);
        assert_eq!(
            result,
            TaskKind::FixError {
                line_number: 0,
                error_text: "I got a traceback in my code".to_string(),
            }
        );
    }

    // Requirement 13.1: review
    #[test]
    fn test_classify_review() {
        let result = classify("review this code", None);
        assert_eq!(result, TaskKind::Review);
    }

    #[test]
    fn test_classify_review_case_insensitive() {
        let result = classify("Please Review my file", None);
        assert_eq!(result, TaskKind::Review);
    }

    // Requirement 13.3: translations (only for .html)
    #[test]
    fn test_classify_translations_html() {
        let result = classify("check translations", Some("html"));
        assert_eq!(result, TaskKind::TranslationCheck);
    }

    #[test]
    fn test_classify_translations_non_html() {
        let result = classify("check translations", Some("py"));
        assert_eq!(
            result,
            TaskKind::GeneralChat {
                input: "check translations".to_string()
            }
        );
    }

    #[test]
    fn test_classify_translations_no_ext() {
        let result = classify("check translations", None);
        assert_eq!(
            result,
            TaskKind::GeneralChat {
                input: "check translations".to_string()
            }
        );
    }

    // Requirement 13.4: add date / add time / add header
    #[test]
    fn test_classify_add_date() {
        let result = classify("add date to the header", None);
        assert_eq!(result, TaskKind::HeaderDateTime);
    }

    #[test]
    fn test_classify_add_time() {
        let result = classify("add time display", None);
        assert_eq!(result, TaskKind::HeaderDateTime);
    }

    #[test]
    fn test_classify_add_header() {
        let result = classify("add header with timestamp", None);
        assert_eq!(result, TaskKind::HeaderDateTime);
    }

    // Requirement 13.9: general chat (fallback)
    #[test]
    fn test_classify_general_chat() {
        let result = classify("how do I use iterators?", None);
        assert_eq!(
            result,
            TaskKind::GeneralChat {
                input: "how do I use iterators?".to_string()
            }
        );
    }

    #[test]
    fn test_classify_general_chat_empty() {
        let result = classify("", None);
        assert_eq!(
            result,
            TaskKind::GeneralChat {
                input: "".to_string()
            }
        );
    }

    // Priority tests: earlier rules take precedence
    #[test]
    fn test_classify_search_takes_priority_over_review() {
        // "/search review" should be Search, not Review
        let result = classify("/search review", None);
        assert_eq!(
            result,
            TaskKind::Search {
                term: "review".to_string()
            }
        );
    }

    #[test]
    fn test_classify_traceback_takes_priority_over_review() {
        // "review this traceback" should be FixError (traceback checked before review)
        let result = classify("review this traceback", None);
        assert_eq!(
            result,
            TaskKind::FixError {
                line_number: 0,
                error_text: "review this traceback".to_string(),
            }
        );
    }

    // Requirement 1.6: /open, /cd, /goto → FileNavigate
    #[test]
    fn test_classify_open() {
        let result = classify("/open src/main.rs", None);
        assert_eq!(
            result,
            TaskKind::FileNavigate {
                path: "src/main.rs".to_string()
            }
        );
    }

    #[test]
    fn test_classify_cd() {
        let result = classify("/cd src/ui", None);
        assert_eq!(
            result,
            TaskKind::FileNavigate {
                path: "src/ui".to_string()
            }
        );
    }

    #[test]
    fn test_classify_goto() {
        let result = classify("/goto config/settings.toml", None);
        assert_eq!(
            result,
            TaskKind::FileNavigate {
                path: "config/settings.toml".to_string()
            }
        );
    }

    #[test]
    fn test_classify_open_case_insensitive() {
        let result = classify("/Open My/Path", None);
        assert_eq!(
            result,
            TaskKind::FileNavigate {
                path: "My/Path".to_string()
            }
        );
    }

    #[test]
    fn test_classify_open_with_leading_whitespace() {
        let result = classify("  /open some/dir  ", None);
        assert_eq!(
            result,
            TaskKind::FileNavigate {
                path: "some/dir".to_string()
            }
        );
    }

    // Requirement 2.7: /create, /new → FileCreate
    #[test]
    fn test_classify_create_with_description() {
        let result = classify("/create nginx.conf reverse proxy for port 8080", None);
        assert_eq!(
            result,
            TaskKind::FileCreate {
                filename: "nginx.conf".to_string(),
                description: "reverse proxy for port 8080".to_string(),
            }
        );
    }

    #[test]
    fn test_classify_create_without_description() {
        let result = classify("/create Dockerfile", None);
        assert_eq!(
            result,
            TaskKind::FileCreate {
                filename: "Dockerfile".to_string(),
                description: "".to_string(),
            }
        );
    }

    #[test]
    fn test_classify_new_with_description() {
        let result = classify("/new setup.sh install script for Python 3.11", None);
        assert_eq!(
            result,
            TaskKind::FileCreate {
                filename: "setup.sh".to_string(),
                description: "install script for Python 3.11".to_string(),
            }
        );
    }

    #[test]
    fn test_classify_new_case_insensitive() {
        let result = classify("/New myfile.txt", None);
        assert_eq!(
            result,
            TaskKind::FileCreate {
                filename: "myfile.txt".to_string(),
                description: "".to_string(),
            }
        );
    }

    // Requirement 3.8: /edit, /insert, /modify → FileEdit
    #[test]
    fn test_classify_edit() {
        let result = classify("/edit nginx.conf add upstream block", None);
        assert_eq!(
            result,
            TaskKind::FileEdit {
                path: "nginx.conf".to_string(),
                instruction: "add upstream block".to_string(),
            }
        );
    }

    #[test]
    fn test_classify_insert() {
        let result = classify("/insert config.yaml add database section", None);
        assert_eq!(
            result,
            TaskKind::FileEdit {
                path: "config.yaml".to_string(),
                instruction: "add database section".to_string(),
            }
        );
    }

    #[test]
    fn test_classify_modify() {
        let result = classify("/modify Dockerfile use python 3.12", None);
        assert_eq!(
            result,
            TaskKind::FileEdit {
                path: "Dockerfile".to_string(),
                instruction: "use python 3.12".to_string(),
            }
        );
    }

    #[test]
    fn test_classify_edit_path_only() {
        let result = classify("/edit src/main.rs", None);
        assert_eq!(
            result,
            TaskKind::FileEdit {
                path: "src/main.rs".to_string(),
                instruction: "".to_string(),
            }
        );
    }

    #[test]
    fn test_classify_edit_case_insensitive() {
        let result = classify("/Edit file.txt fix typo", None);
        assert_eq!(
            result,
            TaskKind::FileEdit {
                path: "file.txt".to_string(),
                instruction: "fix typo".to_string(),
            }
        );
    }

    // Requirement 4.6: /find, /ls → FileFind
    #[test]
    fn test_classify_find() {
        let result = classify("/find *.toml", None);
        assert_eq!(
            result,
            TaskKind::FileFind {
                pattern: "*.toml".to_string()
            }
        );
    }

    #[test]
    fn test_classify_ls() {
        let result = classify("/ls src/**/*.rs", None);
        assert_eq!(
            result,
            TaskKind::FileFind {
                pattern: "src/**/*.rs".to_string()
            }
        );
    }

    #[test]
    fn test_classify_find_case_insensitive() {
        let result = classify("/Find config*", None);
        assert_eq!(
            result,
            TaskKind::FileFind {
                pattern: "config*".to_string()
            }
        );
    }

    #[test]
    fn test_classify_ls_with_whitespace() {
        let result = classify("  /ls *.yaml  ", None);
        assert_eq!(
            result,
            TaskKind::FileFind {
                pattern: "*.yaml".to_string()
            }
        );
    }

    // Priority tests: existing commands take precedence over file ops
    #[test]
    fn test_classify_search_takes_priority_over_file_find() {
        // "/search *.rs" should be Search, not FileFind
        let result = classify("/search *.rs", None);
        assert_eq!(
            result,
            TaskKind::Search {
                term: "*.rs".to_string()
            }
        );
    }

    #[test]
    fn test_classify_run_takes_priority_over_file_edit() {
        // "/run edit something" should be RunCommand, not FileEdit
        let result = classify("/run edit something", None);
        assert_eq!(
            result,
            TaskKind::RunCommand {
                command: "edit something".to_string()
            }
        );
    }

    #[test]
    fn test_classify_file_navigate_takes_priority_over_traceback() {
        // "/open traceback.log" should be FileNavigate, not FixError
        let result = classify("/open traceback.log", None);
        assert_eq!(
            result,
            TaskKind::FileNavigate {
                path: "traceback.log".to_string()
            }
        );
    }

    #[test]
    fn test_classify_file_navigate_takes_priority_over_review() {
        // "/open review.md" should be FileNavigate, not Review
        let result = classify("/open review.md", None);
        assert_eq!(
            result,
            TaskKind::FileNavigate {
                path: "review.md".to_string()
            }
        );
    }

    // Requirement 7.4, 7.5: Natural language file operations

    // NL Navigate
    #[test]
    fn test_classify_nl_navigate_open() {
        let result = classify("open src/main.rs", None);
        assert_eq!(
            result,
            TaskKind::FileNavigate {
                path: "src/main.rs".to_string()
            }
        );
    }

    #[test]
    fn test_classify_nl_navigate_go_to() {
        let result = classify("go to config/settings.toml", None);
        assert_eq!(
            result,
            TaskKind::FileNavigate {
                path: "config/settings.toml".to_string()
            }
        );
    }

    #[test]
    fn test_classify_nl_navigate_navigate_to() {
        let result = classify("navigate to src/ui", None);
        assert_eq!(
            result,
            TaskKind::FileNavigate {
                path: "src/ui".to_string()
            }
        );
    }

    #[test]
    fn test_classify_nl_navigate_cd() {
        let result = classify("cd src/rag", None);
        assert_eq!(
            result,
            TaskKind::FileNavigate {
                path: "src/rag".to_string()
            }
        );
    }

    #[test]
    fn test_classify_nl_navigate_case_insensitive() {
        let result = classify("Open My/Path", None);
        assert_eq!(
            result,
            TaskKind::FileNavigate {
                path: "My/Path".to_string()
            }
        );
    }

    // NL Create
    #[test]
    fn test_classify_nl_create_with_description() {
        let result = classify("create nginx.conf reverse proxy for port 8080", None);
        assert_eq!(
            result,
            TaskKind::FileCreate {
                filename: "nginx.conf".to_string(),
                description: "reverse proxy for port 8080".to_string(),
            }
        );
    }

    #[test]
    fn test_classify_nl_create_without_description() {
        let result = classify("create Dockerfile", None);
        assert_eq!(
            result,
            TaskKind::FileCreate {
                filename: "Dockerfile".to_string(),
                description: "".to_string(),
            }
        );
    }

    #[test]
    fn test_classify_nl_create_with_article() {
        let result = classify("create a setup.sh install script", None);
        assert_eq!(
            result,
            TaskKind::FileCreate {
                filename: "setup.sh".to_string(),
                description: "install script".to_string(),
            }
        );
    }

    #[test]
    fn test_classify_nl_make() {
        let result = classify("make config.yaml with database settings", None);
        assert_eq!(
            result,
            TaskKind::FileCreate {
                filename: "config.yaml".to_string(),
                description: "with database settings".to_string(),
            }
        );
    }

    #[test]
    fn test_classify_nl_generate() {
        let result = classify("generate docker-compose.yml", None);
        assert_eq!(
            result,
            TaskKind::FileCreate {
                filename: "docker-compose.yml".to_string(),
                description: "".to_string(),
            }
        );
    }

    #[test]
    fn test_classify_nl_write() {
        let result = classify("write a Makefile for building the project", None);
        assert_eq!(
            result,
            TaskKind::FileCreate {
                filename: "Makefile".to_string(),
                description: "for building the project".to_string(),
            }
        );
    }

    // NL Edit
    #[test]
    fn test_classify_nl_edit() {
        let result = classify("edit nginx.conf add upstream block", None);
        assert_eq!(
            result,
            TaskKind::FileEdit {
                path: "nginx.conf".to_string(),
                instruction: "add upstream block".to_string(),
            }
        );
    }

    #[test]
    fn test_classify_nl_change() {
        let result = classify("change config.yaml set port to 9090", None);
        assert_eq!(
            result,
            TaskKind::FileEdit {
                path: "config.yaml".to_string(),
                instruction: "set port to 9090".to_string(),
            }
        );
    }

    #[test]
    fn test_classify_nl_add_to() {
        let result = classify("add to Dockerfile a health check", None);
        assert_eq!(
            result,
            TaskKind::FileEdit {
                path: "Dockerfile".to_string(),
                instruction: "a health check".to_string(),
            }
        );
    }

    #[test]
    fn test_classify_nl_update() {
        let result = classify("update package.json bump version", None);
        assert_eq!(
            result,
            TaskKind::FileEdit {
                path: "package.json".to_string(),
                instruction: "bump version".to_string(),
            }
        );
    }

    #[test]
    fn test_classify_nl_insert_into() {
        let result = classify("insert into main.rs a new function", None);
        assert_eq!(
            result,
            TaskKind::FileEdit {
                path: "main.rs".to_string(),
                instruction: "a new function".to_string(),
            }
        );
    }

    #[test]
    fn test_classify_nl_edit_path_only() {
        let result = classify("edit src/main.rs", None);
        assert_eq!(
            result,
            TaskKind::FileEdit {
                path: "src/main.rs".to_string(),
                instruction: "".to_string(),
            }
        );
    }

    // NL Find
    #[test]
    fn test_classify_nl_find() {
        let result = classify("find *.toml", None);
        assert_eq!(
            result,
            TaskKind::FileFind {
                pattern: "*.toml".to_string()
            }
        );
    }

    #[test]
    fn test_classify_nl_locate() {
        let result = classify("locate config files", None);
        assert_eq!(
            result,
            TaskKind::FileFind {
                pattern: "config files".to_string()
            }
        );
    }

    #[test]
    fn test_classify_nl_list_files() {
        let result = classify("list files in src", None);
        assert_eq!(
            result,
            TaskKind::FileFind {
                pattern: "in src".to_string()
            }
        );
    }

    #[test]
    fn test_classify_nl_where_is() {
        let result = classify("where is Cargo.toml", None);
        assert_eq!(
            result,
            TaskKind::FileFind {
                pattern: "Cargo.toml".to_string()
            }
        );
    }

    // Priority: existing keyword checks take precedence over NL file ops
    #[test]
    fn test_classify_review_takes_priority_over_nl_edit() {
        // "review" keyword should match before NL edit pattern
        let result = classify("review this code", None);
        assert_eq!(result, TaskKind::Review);
    }

    #[test]
    fn test_classify_add_date_takes_priority_over_nl_edit() {
        // "add date" keyword should match before NL edit "add to" pattern
        let result = classify("add date to the header", None);
        assert_eq!(result, TaskKind::HeaderDateTime);
    }

    #[test]
    fn test_classify_add_header_takes_priority_over_nl_edit() {
        // "add header" keyword should match before NL edit pattern
        let result = classify("add header with timestamp", None);
        assert_eq!(result, TaskKind::HeaderDateTime);
    }

    #[test]
    fn test_classify_traceback_takes_priority_over_nl_find() {
        // "traceback" keyword should match before NL find "locate" pattern
        let result = classify("I got a traceback error", None);
        assert_eq!(
            result,
            TaskKind::FixError {
                line_number: 0,
                error_text: "I got a traceback error".to_string(),
            }
        );
    }

    #[test]
    fn test_classify_translations_html_takes_priority_over_nl_find() {
        // "translations" keyword with html ext should match before NL patterns
        let result = classify("find translations issues", Some("html"));
        assert_eq!(result, TaskKind::TranslationCheck);
    }
}
