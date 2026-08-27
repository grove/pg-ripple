pub(super) fn tokenize_rules(text: &str) -> Vec<String> {
    let mut rules = Vec::new();
    let mut current = String::new();
    let mut in_literal = false;
    let mut in_iri = false;

    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' if !in_iri => {
                in_literal = !in_literal;
                current.push(c);
            }
            '<' if !in_literal => {
                in_iri = true;
                current.push(c);
            }
            '>' if !in_literal && in_iri => {
                in_iri = false;
                current.push(c);
            }
            '.' if !in_literal && !in_iri => {
                let trimmed = current.trim().to_owned();
                if !trimmed.is_empty() {
                    rules.push(trimmed);
                }
                current.clear();
            }
            '#' if !in_literal && !in_iri => {
                // Line comment — skip until end of line.
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            _ => current.push(c),
        }
        i += 1;
    }
    let trimmed = current.trim().to_owned();
    if !trimmed.is_empty() {
        rules.push(trimmed);
    }
    rules
}

// ─── Rule parser ─────────────────────────────────────────────────────────────

/// Parse a single rule (without the trailing `.`).
pub(super) fn split_body(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_literal = false;
    let mut in_iri = false;

    for c in text.chars() {
        match c {
            '"' => {
                in_literal = !in_literal;
                current.push(c);
            }
            '<' if !in_literal => {
                in_iri = true;
                current.push(c);
            }
            '>' if !in_literal && in_iri => {
                in_iri = false;
                current.push(c);
            }
            '{' | '(' if !in_literal && !in_iri => {
                depth += 1;
                current.push(c);
            }
            '}' | ')' if !in_literal && !in_iri => {
                depth -= 1;
                current.push(c);
            }
            ',' if !in_literal && !in_iri && depth == 0 => {
                parts.push(current.trim().to_owned());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_owned());
    }
    parts
}

/// Parse a single body literal.
pub(super) fn split_csv(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_str = false;
    for c in s.chars() {
        match c {
            '\'' => {
                in_str = !in_str;
                current.push(c);
            }
            '(' if !in_str => {
                depth += 1;
                current.push(c);
            }
            ')' if !in_str => {
                depth -= 1;
                current.push(c);
            }
            ',' if !in_str && depth == 0 => {
                parts.push(current.clone());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() || !parts.is_empty() {
        parts.push(current);
    }
    parts
}

/// Try parsing an aggregate body literal.
///
/// Syntax: `COUNT(?aggVar WHERE subject pred object) = ?resultVar`
/// Also supports SUM, MIN, MAX, AVG.
pub(super) fn tokenize_terms(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_literal = false;
    let mut in_iri = false;
    let mut in_quoted = false; // << >> quoted triple

    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                in_literal = !in_literal;
                current.push(c);
            }
            '<' if !in_literal => {
                // Check for <<
                if i + 1 < chars.len() && chars[i + 1] == '<' {
                    in_quoted = true;
                    current.push(c);
                    current.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                in_iri = true;
                current.push(c);
            }
            '>' if !in_literal && in_quoted => {
                // Check for >>
                if i + 1 < chars.len() && chars[i + 1] == '>' {
                    in_quoted = false;
                    current.push(c);
                    current.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                current.push(c);
            }
            '>' if !in_literal && in_iri => {
                in_iri = false;
                current.push(c);
            }
            ' ' | '\t' | '\n' if !in_literal && !in_iri && !in_quoted => {
                if !current.is_empty() {
                    tokens.push(current.trim().to_owned());
                    current.clear();
                }
            }
            _ => current.push(c),
        }
        i += 1;
    }
    if !current.trim().is_empty() {
        tokens.push(current.trim().to_owned());
    }
    tokens
}
