#[derive(Debug, thiserror::Error)]
pub enum ClickUpError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("API error {status}{}: {message}", code.as_deref().map(|c| format!(" [{c}]")).unwrap_or_default())]
    Api {
        status: u16,
        code: Option<String>,
        message: String,
        trace_id: Option<String>,
    },

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Rate limited until {reset_at}")]
    RateLimit { reset_at: i64 },

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Could not resolve {kind} '{input}'{}", suggestion_hint(.suggestions))]
    Resolve {
        kind: &'static str,
        input: String,
        suggestions: Vec<String>,
    },

    #[error("Invalid input: {0}")]
    Invalid(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ClickUpError>;

fn suggestion_hint(suggestions: &[String]) -> String {
    if suggestions.is_empty() {
        String::new()
    } else {
        format!(" (did you mean: {}?)", suggestions.join(", "))
    }
}

impl ClickUpError {
    pub fn resolve(kind: &'static str, input: impl Into<String>, suggestions: Vec<String>) -> Self {
        Self::Resolve {
            kind,
            input: input.into(),
            suggestions,
        }
    }
}

/// Replace anything that looks like a personal token (`pk_…`) with `pk_***`.
pub fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("pk_") {
        out.push_str(&rest[..pos]);
        out.push_str("pk_***");
        let tail = &rest[pos + 3..];
        let end = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(tail.len());
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_tokens() {
        assert_eq!(redact("token pk_123_ABC end"), "token pk_*** end");
        assert_eq!(redact("pk_a,pk_b"), "pk_***,pk_***");
        assert_eq!(redact("nothing here"), "nothing here");
    }

    #[test]
    fn resolve_message_has_suggestions() {
        let e = ClickUpError::resolve("list", "Sprnt", vec!["Sprint".into()]);
        assert_eq!(
            e.to_string(),
            "Could not resolve list 'Sprnt' (did you mean: Sprint?)"
        );
    }
}
