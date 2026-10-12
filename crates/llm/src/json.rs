//! Find a JSON object in model text: models without a JSON mode wrap it in prose or ``` fences.

use serde_json::Value;

/// The first JSON object (or array) in `text` that parses, trying fenced blocks first.
pub fn extract_json(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    if let Ok(v) = serde_json::from_str::<Value>(trimmed)
        && (v.is_object() || v.is_array())
    {
        return Some(v);
    }
    // ```json … ``` blocks.
    let mut rest = text;
    while let Some(start) = rest.find("```") {
        let after = &rest[start + 3..];
        let body_start = after.find('\n').map_or(0, |i| i + 1);
        let Some(end) = after[body_start..].find("```") else {
            break;
        };
        let body = &after[body_start..body_start + end];
        if let Ok(v) = serde_json::from_str::<Value>(body.trim()) {
            return Some(v);
        }
        rest = &after[body_start + end + 3..];
    }
    // Balanced braces anywhere, string-aware.
    let bytes = text.as_bytes();
    for (start, &b) in bytes.iter().enumerate() {
        if b != b'{' && b != b'[' {
            continue;
        }
        if let Some(end) = balanced_end(text, start)
            && let Ok(v) = serde_json::from_str::<Value>(&text[start..=end])
        {
            return Some(v);
        }
    }
    None
}

fn balanced_end(text: &str, start: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in text[start..].char_indices() {
        if in_string {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' | '[' => depth += 1,
            '}' | ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + i);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_json_in_prose_and_fences() {
        assert_eq!(extract_json(r#"{"a":1}"#), Some(json!({"a": 1})));
        assert_eq!(
            extract_json("Sure!\n```json\n{\"query\": \"g.V()\"}\n```\nDone."),
            Some(json!({"query": "g.V()"}))
        );
        assert_eq!(
            extract_json(r#"Here you go: {"q": "has('a', '}')", "n": [1,2]} hope it helps"#),
            Some(json!({"q": "has('a', '}')", "n": [1, 2]}))
        );
        assert_eq!(extract_json("no json here"), None);
    }
}
