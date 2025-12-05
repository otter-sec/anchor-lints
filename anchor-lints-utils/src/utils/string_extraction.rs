use clippy_utils::source::HasSession;
use rustc_lint::LateContext;
use rustc_span::Span;

// Remove comments from a code snippet
pub fn remove_comments(code: &str) -> String {
    code.lines()
        .filter(|line| !line.trim().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

// Extracts account name from a code snippet matching the pattern `accounts.<name>` or standalone `name`.
pub fn extract_account_name_from_string(s: &str) -> Option<String> {
    let s = s.trim_start_matches("&mut ").trim_start_matches("& ");

    if let Some(after_accounts) = s.split(".accounts.").nth(1) {
        let account_name: String = after_accounts
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !account_name.is_empty() {
            return Some(account_name);
        }
    }

    if let Some(accounts_pos) = s.find("accounts.")
        && (accounts_pos == 0 || s[..accounts_pos].ends_with('.'))
    {
        let after_accounts = &s[accounts_pos + "accounts.".len()..];
        let account_name: String = after_accounts
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !account_name.is_empty() {
            return Some(account_name);
        }
    }

    let dot_count = s.matches('.').count();
    match dot_count {
        1 => {
            if let Some(dot_pos) = s.find('.') {
                let before_dot = &s[..dot_pos];
                let account_name: String = before_dot
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !account_name.is_empty() {
                    return Some(account_name);
                }
            }
        }
        2 => {
            if let Some(last_dot_pos) = s.rfind('.') {
                let after_last_dot = &s[last_dot_pos + 1..];
                let account_name: String = after_last_dot
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !account_name.is_empty() {
                    return Some(account_name);
                }
            }
        }
        _ => {}
    }

    if !s.is_empty() {
        Some(s.to_string())
    } else {
        None
    }
}

// Extracts the account name from a code snippet matching the pattern `accounts.<name>` or standalone `name`.
pub fn extract_context_account(line: &str, return_only_name: bool) -> Option<String> {
    let snippet = remove_comments(line);
    let snippet = snippet.trim_start_matches("&mut ").trim_start_matches("& ");

    if let Some(start) = snippet.find(".accounts.") {
        let prefix_start = snippet[..start]
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| i + 1)
            .unwrap_or(0);
        let prefix = &snippet[prefix_start..start];

        let rest = &snippet[start + ".accounts.".len()..];
        let account_name_end = rest
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(rest.len());
        let account = &rest[..account_name_end];

        if return_only_name {
            Some(account.to_string())
        } else {
            Some(format!("{}.accounts.{}", prefix, account))
        }
    } else {
        extract_account_name_from_string(snippet)
    }
}

// Extracts the elements of a vec from a code snippet.
pub fn extract_vec_elements(snippet: &str) -> Vec<String> {
    let mut trimmed = snippet.trim();
    trimmed = trimmed
        .trim_start_matches('&')
        .trim_start_matches("mut ")
        .trim();

    // Find the actual vec! macro even if preceded by `let ... =`
    let (pos, open, close) = if let Some(idx) = trimmed.find("vec![") {
        (idx, "vec![", ']')
    } else if let Some(idx) = trimmed.find("vec!(") {
        (idx, "vec!(", ')')
    } else {
        return Vec::new();
    };

    let after_open = &trimmed[pos + open.len()..];

    // Find the matching closing bracket for this vec![] by tracking bracket depth
    let mut depth = 1; // We're already inside the opening bracket
    let mut close_pos = None;

    for (i, ch) in after_open.char_indices() {
        match ch {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' if ch == close => {
                depth -= 1;
                if depth == 0 {
                    close_pos = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }

    // Extract only the inner content up to the matching closing bracket
    let inner = if let Some(close_idx) = close_pos {
        &after_open[..close_idx]
    } else {
        // Fallback: try to trim end if we can't find matching bracket
        after_open
            .trim_end_matches(';')
            .trim_end_matches(close)
            .trim()
    };

    let mut elements = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for ch in inner.chars() {
        match ch {
            '[' | '(' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ']' | ')' | '}' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                if !current.trim().is_empty() {
                    elements.push(current.trim().to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    // Add the last element
    if !current.trim().is_empty() {
        elements.push(current.trim().to_string());
    }

    elements
}

pub fn extract_vec_snippet_from_span(cx: &LateContext<'_>, span: Span) -> Option<String> {
    let file_lines = cx.sess().source_map().span_to_lines(span).ok()?;
    let file = &file_lines.file;
    let start = file_lines.lines[0].line_index;

    let src = file.src.as_ref()?;
    let lines: Vec<&str> = src.lines().collect();

    let mut buf = String::new();
    let mut depth = 0;
    let mut seen_open = false;

    for line in lines.iter().skip(start) {
        buf.push_str(line);
        buf.push('\n');

        for ch in line.chars() {
            match ch {
                '[' | '(' | '{' => {
                    if !seen_open {
                        seen_open = true;
                    }
                    depth += 1;
                }
                ']' | ')' | '}' => {
                    if depth > 0 {
                        depth -= 1;
                    }
                }
                _ => {}
            }
        }

        if seen_open && depth == 0 {
            break;
        }
    }

    Some(buf)
}
