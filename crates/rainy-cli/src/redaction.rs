use serde_json::Value;

const SENSITIVE_KEYS: &[&str] = &[
    "authorization",
    "cookie",
    "credential",
    "password",
    "privatekey",
    "secret",
    "secretkey",
    "signature",
    "token",
];

pub fn text(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut token = String::new();
    let mut redact_next = false;
    for character in value.chars() {
        if character.is_whitespace() {
            if !token.is_empty() {
                let rendered = if redact_next {
                    "[REDACTED]".to_string()
                } else {
                    redact_token(&token)
                };
                output.push_str(&rendered);
                redact_next = !redact_next && sensitive_flag(&token);
                token.clear();
            }
            output.push(character);
        } else {
            token.push(character);
        }
    }
    if !token.is_empty() {
        let rendered = if redact_next {
            "[REDACTED]".to_string()
        } else {
            redact_token(&token)
        };
        output.push_str(&rendered);
    }
    output
}

pub fn json(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if sensitive_key(key) {
                    *child = Value::String("[REDACTED]".to_string());
                } else {
                    json(child);
                }
            }
        }
        Value::Array(values) => values.iter_mut().for_each(json),
        Value::String(value) => *value = text(value),
        _ => {}
    }
}

fn sensitive_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    SENSITIVE_KEYS
        .iter()
        .any(|candidate| normalized.contains(candidate))
}

fn redact_token(token: &str) -> String {
    let lower = token.to_ascii_lowercase();
    if lower.starts_with("authorization:")
        || lower.starts_with("bearer=")
        || lower.starts_with("password=")
        || lower.starts_with("secret=")
        || lower.starts_with("token=")
    {
        if let Some((name, _)) = token.split_once('=') {
            return format!("{name}=[REDACTED]");
        }
        if let Some((name, _)) = token.split_once(':') {
            return format!("{name}:[REDACTED]");
        }
        return "[REDACTED]".to_string();
    }
    if let Some((name, _)) = token.split_once('=')
        && sensitive_flag(name)
    {
        return format!("{name}=[REDACTED]");
    }
    if let Ok(mut url) = url::Url::parse(token) {
        let mut changed = false;
        if !url.username().is_empty() || url.password().is_some() {
            let _ = url.set_username("[REDACTED]");
            let _ = url.set_password(None);
            changed = true;
        }
        let pairs = url
            .query_pairs()
            .map(|(key, value)| {
                if sensitive_flag(&key) {
                    changed = true;
                    (key.into_owned(), "[REDACTED]".to_string())
                } else {
                    (key.into_owned(), value.into_owned())
                }
            })
            .collect::<Vec<_>>();
        if changed {
            url.query_pairs_mut().clear().extend_pairs(pairs);
            return url.to_string();
        }
    }
    token.to_string()
}

fn sensitive_flag(value: &str) -> bool {
    let normalized = value
        .trim_matches(|character: char| !character.is_ascii_alphanumeric())
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    SENSITIVE_KEYS
        .iter()
        .any(|candidate| normalized.contains(candidate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_credentials_from_text_and_json() {
        assert!(!text("https://user:secret@example.com/repo").contains("secret"));
        assert!(!text("command --token hunter2 --password=secret").contains("hunter2"));
        assert!(!text("command --token hunter2 --password=secret").contains("secret"));
        assert!(!text("https://example.com/file?token=hunter2").contains("hunter2"));
        let mut value = serde_json::json!({"token": "abc", "message": "password=hunter2"});
        json(&mut value);
        assert_eq!(value["token"], "[REDACTED]");
        assert!(!value["message"].as_str().unwrap().contains("hunter2"));
    }
}
