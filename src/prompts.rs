use crate::rag::SearchHit;

// Prompt template functions

/// A message to send to the LLM, consisting of a role and content.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }
}

/// Build a code review prompt for the given file content.
///
/// Returns a system message instructing the LLM to review code for bugs,
/// improvements, and best practices, plus a user message with the file content.
pub fn review_prompt(filename: &str, content: &str) -> Vec<Message> {
    vec![
        Message::system(
            "You are a concise senior coding assistant. \
             Review the provided code for bugs, improvements, and best practices. \
             For each issue, explain WHY it's a problem. \
             If you propose changes, use unified diff format.",
        ),
        Message::user(format!(
            "Review the following file for code quality, potential bugs, \
             and improvements.\n\n\
             File: {filename}\n\
             ```\n{content}\n```\n\n\
             Provide a concise review covering:\n\
             - Bugs or logic errors\n\
             - Style and readability issues\n\
             - Performance concerns\n\
             - Security issues\n\n\
             For each issue, explain WHY it's a problem. \
             If you propose changes, use unified diff format."
        )),
    ]
}

/// Build a fix-error prompt with snippet and error context.
///
/// Returns a system message instructing the LLM to provide a unified diff fix,
/// plus a user message with the code snippet and error description.
pub fn fix_error_prompt(filename: &str, snippet: &str, error_text: &str) -> Vec<Message> {
    vec![
        Message::system(
            "You are a concise senior coding assistant. \
             Fix the described error. First explain the root cause in 1-2 sentences, \
             then provide the fix as a unified diff.",
        ),
        Message::user(format!(
            "Fix the error in the following code.\n\n\
             File: {filename}\n\
             Error: {error_text}\n\n\
             Code snippet (with line numbers):\n\
             ```\n{snippet}\n```\n\n\
             First explain the root cause of the error in 1-2 sentences, \
             then provide the fix as a unified diff."
        )),
    ]
}

/// Build a Django template translation check prompt.
///
/// Returns a system message instructing the LLM to find untranslated strings
/// in Django templates, plus a user message with the HTML content.
pub fn translation_check_prompt(filename: &str, content: &str) -> Vec<Message> {
    vec![
        Message::system(
            "You are a concise senior coding assistant specializing in Django i18n. \
             Identify visible text that is NOT wrapped in {% trans %}, {% blocktrans %}, \
             or gettext calls. Ignore HTML tags, attributes, URLs, CSS classes, IDs, \
             and JavaScript/CSS blocks. For each finding, explain WHY it needs translation. \
             Propose changes as a unified diff.",
        ),
        Message::user(format!(
            "Analyze the following Django HTML template for untranslated strings.\n\n\
             File: {filename}\n\
             ```html\n{content}\n```\n\n\
             Identify visible text that is NOT wrapped in {{% trans %}}, \
             {{% blocktrans %}}, or gettext calls.\n\
             Ignore HTML tags, attributes, URLs, CSS classes, IDs, and \
             JavaScript/CSS blocks.\n\n\
             For each finding, explain WHY it needs translation. \
             Then propose a unified diff converting plain text to \
             {{% trans \"Text\" %}} format."
        )),
    ]
}

/// Build a prompt to add date/time display to a template header.
///
/// Returns a system message instructing the LLM to add date/time to a header,
/// plus a user message with the file content.
pub fn header_datetime_prompt(filename: &str, content: &str) -> Vec<Message> {
    vec![
        Message::system(
            "You are a concise senior coding assistant. \
             Add a date/time display to the header of the provided template. \
             For Django templates, prefer the {% now \"Y-m-d H:i\" %} tag. \
             Explain where you're placing it and why, then provide \
             a minimal unified diff with the change.",
        ),
        Message::user(format!(
            "Add a date/time display to the header of this template.\n\n\
             File: {filename}\n\
             ```html\n{content}\n```\n\n\
             For Django templates, prefer the {{% now \"Y-m-d H:i\" %}} tag.\n\
             Explain where you're placing it and why, then provide \
             a minimal unified diff with the change."
        )),
    ]
}

/// Build a general coding question prompt with file context.
///
/// Returns a system message saying you're a coding and system assistant, plus a user
/// message with the file content and the question.
pub fn general_chat_prompt(filename: &str, content: &str, question: &str) -> Vec<Message> {
    vec![
        Message::system(
            "You are a concise senior coding and system administration assistant. \
             You help with programming, configuration files, server management, \
             shell scripting, and DevOps tasks. \
             Provide direct, actionable answers. When proposing file changes, \
             always explain WHY you suggest the change in 1-2 sentences first, \
             then provide the change as a unified diff.",
        ),
        Message::user(format!(
            "Answer the following question about this file.\n\n\
             File: {filename}\n\
             ```\n{content}\n```\n\n\
             Question: {question}\n\n\
             If your answer involves file changes, first explain WHY the change \
             is needed, then provide the changes as a unified diff."
        )),
    ]
}

/// Build a general question prompt without file context.
///
/// Returns a system message saying you're a coding and system assistant, plus a user
/// message with just the question.
pub fn general_chat_prompt_no_file(question: &str) -> Vec<Message> {
    vec![
        Message::system(
            "You are a concise senior coding and system administration assistant. \
             You help with programming, configuration files, server management, \
             shell scripting, and DevOps tasks. \
             Provide direct, actionable answers. When proposing file changes, \
             always explain WHY you suggest the change in 1-2 sentences first, \
             then provide the change as a unified diff. \
             When asked to run commands, provide the exact shell commands needed.",
        ),
        Message::user(question.to_string()),
    ]
}

/// Detect the file type from a filename and return type-specific generation instructions.
fn file_type_instructions(filename: &str) -> &'static str {
    let lower = filename.to_lowercase();

    // Nginx configuration
    if lower.contains("nginx") && lower.ends_with(".conf") {
        return "\n\nThis is an nginx configuration file. Include:\n\
            - Appropriate worker_processes and worker_connections settings\n\
            - Security headers (X-Frame-Options, X-Content-Type-Options, X-XSS-Protection, Content-Security-Policy)\n\
            - SSL/TLS best practices if HTTPS is relevant\n\
            - Proper logging configuration\n\
            - Gzip compression settings\n\
            - Comments explaining each server block and location directive";
    }

    // Dockerfile
    if lower == "dockerfile" || lower.starts_with("dockerfile.") {
        return "\n\nThis is a Dockerfile. Include:\n\
            - Multi-stage build where appropriate to minimize image size\n\
            - Pinned base image versions (avoid :latest)\n\
            - Non-root user for running the application\n\
            - Minimal layers by combining RUN commands where logical\n\
            - COPY before RUN for better layer caching\n\
            - A HEALTHCHECK instruction if applicable\n\
            - Comments explaining each stage and significant instruction";
    }

    // Shell scripts
    if lower.ends_with(".sh") || lower.ends_with(".bash") || lower.ends_with(".zsh") {
        return "\n\nThis is a shell script. Include:\n\
            - Appropriate shebang line (#!/usr/bin/env bash or #!/bin/sh)\n\
            - set -euo pipefail for robust error handling\n\
            - Usage comments at the top explaining purpose and arguments\n\
            - Input validation for any expected arguments\n\
            - Meaningful variable names and quoting of variables\n\
            - Comments explaining non-obvious logic";
    }

    // Systemd unit files
    if lower.ends_with(".service") || lower.ends_with(".timer") || lower.ends_with(".socket") {
        return "\n\nThis is a systemd unit file. Include:\n\
            - Proper [Unit], [Service]/[Timer]/[Socket], and [Install] sections\n\
            - Description and After/Wants dependencies in [Unit]\n\
            - Appropriate Type= directive (simple, forking, oneshot, etc.)\n\
            - Security hardening directives (ProtectSystem, ProtectHome, NoNewPrivileges)\n\
            - Restart policy and resource limits where appropriate\n\
            - Comments explaining key directives";
    }

    // YAML config files (docker-compose, k8s, CI/CD)
    if lower.ends_with(".yml") || lower.ends_with(".yaml") {
        return "\n\nThis is a YAML configuration file. Include:\n\
            - Proper indentation (2 spaces)\n\
            - Comments explaining key sections and non-obvious values\n\
            - Version specifiers where applicable";
    }

    // TOML config files
    if lower.ends_with(".toml") {
        return "\n\nThis is a TOML configuration file. Include:\n\
            - Proper section headers\n\
            - Comments explaining key settings\n\
            - Sensible default values";
    }

    // Generic/unknown file type
    ""
}

/// Build a prompt for generating file content.
///
/// Accepts a filename, description of what the file should contain, and project
/// context (working directory name, existing files summary). Returns a system
/// message with production-quality generation instructions (including type-specific
/// guidance for known file types) and a user message with the creation request.
pub fn file_create_prompt(
    filename: &str,
    description: &str,
    project_context: &str,
) -> Vec<Message> {
    let type_instructions = file_type_instructions(filename);

    let system_content = format!(
        "You are a senior systems and software engineer. \
         Generate production-quality file content that is ready to use with minimal adjustment. \
         Follow these guidelines:\n\
         - Include inline comments explaining key settings and decisions\n\
         - Use best-practice defaults appropriate for the file type\n\
         - Include appropriate headers, shebangs, or preambles as needed\n\
         - Output ONLY the raw file content with no markdown fencing, no explanation, no preamble\n\
         - The content should be complete and functional{type_instructions}"
    );

    let user_content = format!(
        "Create the file: {filename}\n\n\
         Description: {description}\n\n\
         Project context:\n{project_context}\n\n\
         Generate the complete file content. Output only the raw file content, \
         no markdown code blocks or surrounding text."
    );

    vec![Message::system(system_content), Message::user(user_content)]
}

/// Build a prompt for editing an existing file.
///
/// Accepts a filename, the current file content, and an edit instruction describing
/// the desired changes. Returns a system message instructing the LLM to apply the edit
/// and output only the complete modified file content (for diff generation), plus a user
/// message with the filename, current content, and instruction.
pub fn file_edit_prompt(filename: &str, current_content: &str, instruction: &str) -> Vec<Message> {
    let system_content = "\
        You are a senior systems and software engineer. \
        Apply the requested edit to the provided file. \
        Follow these guidelines:\n\
        - Output ONLY the complete modified file content with no markdown fencing, no explanation, no preamble\n\
        - Preserve the existing style, formatting, and indentation of the file\n\
        - Make minimal changes to accomplish the instruction\n\
        - Do not remove or alter code unrelated to the requested change\n\
        - Ensure the result is syntactically valid for the file type";

    let user_content = format!(
        "Edit the file: {filename}\n\n\
         Instruction: {instruction}\n\n\
         Current file content:\n{current_content}\n\n\
         Apply the instruction and output only the complete modified file content. \
         No markdown code blocks or surrounding text."
    );

    vec![Message::system(system_content), Message::user(user_content)]
}

/// Build a RAG-augmented user prompt that includes retrieved context before the query.
///
/// If hits are non-empty, formats each hit's relative_path and content as context.
/// If hits are empty, returns the query with a note that no indexed matches were found.
pub fn build_rag_augmented_prompt(query: &str, hits: &[SearchHit]) -> String {
    if hits.is_empty() {
        return format!(
            "{}\n\n(Note: No indexed matches were found in the project.)",
            query
        );
    }

    let mut prompt = String::from("Based on the following relevant context from the project:\n\n");

    for (i, hit) in hits.iter().enumerate() {
        prompt.push_str(&format!(
            "--- Context {} (from {}) ---\n{}\n\n",
            i + 1,
            hit.metadata.relative_path,
            hit.content
        ));
    }

    prompt.push_str(&format!("Question: {}", query));
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rag::{DocumentMetadata, SearchHit};
    use std::path::PathBuf;

    #[test]
    fn test_message_system() {
        let msg = Message::system("hello");
        assert_eq!(msg.role, "system");
        assert_eq!(msg.content, "hello");
    }

    #[test]
    fn test_message_user() {
        let msg = Message::user("world");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "world");
    }

    #[test]
    fn test_review_prompt_returns_two_messages() {
        let messages = review_prompt("main.py", "print('hello')");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].role, "user");
        assert!(messages[1].content.contains("main.py"));
        assert!(messages[1].content.contains("print('hello')"));
    }

    #[test]
    fn test_fix_error_prompt_returns_two_messages() {
        let messages = fix_error_prompt("app.rs", "let x = 1;", "undefined variable");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].role, "user");
        assert!(messages[1].content.contains("app.rs"));
        assert!(messages[1].content.contains("let x = 1;"));
        assert!(messages[1].content.contains("undefined variable"));
    }

    #[test]
    fn test_translation_check_prompt_returns_two_messages() {
        let messages = translation_check_prompt("base.html", "<h1>Hello</h1>");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].role, "user");
        assert!(messages[1].content.contains("base.html"));
        assert!(messages[1].content.contains("<h1>Hello</h1>"));
    }

    #[test]
    fn test_header_datetime_prompt_returns_two_messages() {
        let messages = header_datetime_prompt("header.html", "<header>Site</header>");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].role, "user");
        assert!(messages[1].content.contains("header.html"));
        assert!(messages[1].content.contains("<header>Site</header>"));
    }

    #[test]
    fn test_general_chat_prompt_returns_two_messages() {
        let messages = general_chat_prompt(
            "utils.py",
            "def add(a, b): return a + b",
            "What does this do?",
        );
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].role, "user");
        assert!(messages[1].content.contains("utils.py"));
        assert!(messages[1].content.contains("def add(a, b): return a + b"));
        assert!(messages[1].content.contains("What does this do?"));
    }

    #[test]
    fn test_general_chat_prompt_no_file_returns_two_messages() {
        let messages = general_chat_prompt_no_file("How do I sort a vector in Rust?");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].role, "user");
        assert_eq!(messages[1].content, "How do I sort a vector in Rust?");
    }

    #[test]
    fn test_review_prompt_system_mentions_review() {
        let messages = review_prompt("test.rs", "fn main() {}");
        assert!(messages[0].content.to_lowercase().contains("review"));
    }

    #[test]
    fn test_fix_error_prompt_system_mentions_diff() {
        let messages = fix_error_prompt("test.rs", "code", "error");
        assert!(messages[0].content.to_lowercase().contains("diff"));
    }

    #[test]
    fn test_translation_check_prompt_system_mentions_translation() {
        let messages = translation_check_prompt("t.html", "<p>Hi</p>");
        assert!(messages[0].content.contains("trans"));
    }

    #[test]
    fn test_header_datetime_prompt_system_mentions_datetime() {
        let messages = header_datetime_prompt("h.html", "<header></header>");
        assert!(messages[0].content.contains("date/time"));
    }

    #[test]
    fn test_general_chat_prompt_no_file_question_is_user_content() {
        let q = "Explain closures";
        let messages = general_chat_prompt_no_file(q);
        assert_eq!(messages[1].content, q);
    }

    fn make_search_hit(relative_path: &str, content: &str, score: f64) -> SearchHit {
        SearchHit {
            document_id: 1,
            path: PathBuf::from(relative_path),
            content: content.to_string(),
            score,
            metadata: DocumentMetadata {
                file_name: relative_path
                    .split('/')
                    .last()
                    .unwrap_or(relative_path)
                    .to_string(),
                relative_path: relative_path.to_string(),
                parent_dir: "src".to_string(),
                is_directory: false,
                line_start: Some(1),
                line_end: Some(10),
                chunk_index: Some(0),
            },
        }
    }

    #[test]
    fn test_rag_augmented_prompt_no_hits_includes_note() {
        let result = build_rag_augmented_prompt("where is config?", &[]);
        assert!(result.contains("where is config?"));
        assert!(result.contains("No indexed matches were found"));
    }

    #[test]
    fn test_rag_augmented_prompt_with_hits_includes_paths_and_content() {
        let hits = vec![
            make_search_hit("src/config.rs", "pub struct AppConfig { ... }", 0.9),
            make_search_hit("src/main.rs", "fn main() { ... }", 0.7),
        ];
        let result = build_rag_augmented_prompt("where is config?", &hits);
        assert!(result.contains("src/config.rs"));
        assert!(result.contains("pub struct AppConfig { ... }"));
        assert!(result.contains("src/main.rs"));
        assert!(result.contains("fn main() { ... }"));
        assert!(result.contains("Question: where is config?"));
    }

    #[test]
    fn test_rag_augmented_prompt_with_hits_numbers_context() {
        let hits = vec![
            make_search_hit("src/a.rs", "content a", 0.9),
            make_search_hit("src/b.rs", "content b", 0.8),
        ];
        let result = build_rag_augmented_prompt("query", &hits);
        assert!(result.contains("Context 1"));
        assert!(result.contains("Context 2"));
    }

    #[test]
    fn test_rag_augmented_prompt_single_hit() {
        let hits = vec![make_search_hit("lib/utils.rs", "fn helper() {}", 0.95)];
        let result = build_rag_augmented_prompt("what does helper do?", &hits);
        assert!(result.contains("lib/utils.rs"));
        assert!(result.contains("fn helper() {}"));
        assert!(result.contains("Question: what does helper do?"));
        assert!(!result.contains("No indexed matches"));
    }
}
