//! Parse a task reference: id, custom id (`DEV-123`), or task URL.

use std::{fmt, str::FromStr};

use crate::{client::Query, error::ClickUpError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskRef {
    Id(String),
    Custom(String),
}

fn is_custom_id(s: &str) -> bool {
    let Some((prefix, num)) = s.rsplit_once('-') else {
        return false;
    };
    let mut chars = prefix.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !num.is_empty()
        && num.chars().all(|c| c.is_ascii_digit())
}

fn is_task_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric())
}

fn classify(s: &str) -> Option<TaskRef> {
    if is_custom_id(s) {
        Some(TaskRef::Custom(s.to_string()))
    } else if is_task_id(s) {
        Some(TaskRef::Id(s.to_string()))
    } else {
        None
    }
}

impl FromStr for TaskRef {
    type Err = ClickUpError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let s = input.trim();
        let s = s.strip_prefix('#').unwrap_or(s);
        let err = || {
            ClickUpError::Invalid(format!(
                "'{input}' is not a task id, custom id (DEV-123) or ClickUp task URL"
            ))
        };

        if s.starts_with("http://") || s.starts_with("https://") {
            let url = url::Url::parse(s).map_err(|_| err())?;
            if !url.host_str().is_some_and(|h| h.ends_with("clickup.com")) {
                return Err(err());
            }
            let segs: Vec<&str> = url
                .path_segments()
                .map(|p| p.filter(|x| !x.is_empty()).collect())
                .unwrap_or_default();
            return match segs.as_slice() {
                ["t", id] => classify(id).ok_or_else(err),
                ["t", _team, id] => classify(id).ok_or_else(err),
                _ => Err(err()),
            };
        }

        classify(s).ok_or_else(err)
    }
}

impl fmt::Display for TaskRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.path_id())
    }
}

impl TaskRef {
    pub fn path_id(&self) -> &str {
        match self {
            TaskRef::Id(s) | TaskRef::Custom(s) => s,
        }
    }

    pub fn is_custom(&self) -> bool {
        matches!(self, TaskRef::Custom(_))
    }

    /// `custom_task_ids=true&team_id=…` for custom ids, empty otherwise.
    pub fn query(&self, workspace_id: &str) -> Query {
        match self {
            TaskRef::Custom(_) => Query::new()
                .push("custom_task_ids", "true")
                .push("team_id", workspace_id),
            TaskRef::Id(_) => Query::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Result<TaskRef, ClickUpError> {
        s.parse()
    }

    #[test]
    fn plain_ids() {
        assert_eq!(p("86b1abcde").unwrap(), TaskRef::Id("86b1abcde".into()));
        assert_eq!(p("  9hz ").unwrap(), TaskRef::Id("9hz".into()));
        assert_eq!(p("#86b1abcde").unwrap(), TaskRef::Id("86b1abcde".into()));
    }

    #[test]
    fn custom_ids() {
        assert_eq!(p("DEV-123").unwrap(), TaskRef::Custom("DEV-123".into()));
        assert_eq!(p("eng2-42").unwrap(), TaskRef::Custom("eng2-42".into()));
        assert_eq!(p("#DEV-1").unwrap(), TaskRef::Custom("DEV-1".into()));
        assert_eq!(p("MY_TEAM-7").unwrap(), TaskRef::Custom("MY_TEAM-7".into()));
    }

    #[test]
    fn urls() {
        assert_eq!(
            p("https://app.clickup.com/t/86b1abcde").unwrap(),
            TaskRef::Id("86b1abcde".into())
        );
        assert_eq!(
            p("https://app.clickup.com/t/9012345678/DEV-123").unwrap(),
            TaskRef::Custom("DEV-123".into())
        );
        assert_eq!(
            p("https://app.clickup.com/t/86b1abcde/").unwrap(),
            TaskRef::Id("86b1abcde".into())
        );
        assert_eq!(
            p("https://app.clickup.com/t/86b1abcde?comment=1").unwrap(),
            TaskRef::Id("86b1abcde".into())
        );
    }

    #[test]
    fn garbage() {
        for bad in [
            "",
            "#",
            "-123",
            "DEV-",
            "1-2-3 x",
            "has space",
            "https://example.com/t/abc",
            "https://app.clickup.com/v/l/abc",
            "https://app.clickup.com/t/",
            "abc/def",
            "DEV-12a",
        ] {
            assert!(p(bad).is_err(), "{bad:?} should fail");
        }
    }

    #[test]
    fn query_only_for_custom() {
        assert!(p("abc").unwrap().query("9").pairs().is_empty());
        let q = p("DEV-1").unwrap().query("9");
        assert_eq!(q.get("custom_task_ids"), Some("true"));
        assert_eq!(q.get("team_id"), Some("9"));
    }
}
