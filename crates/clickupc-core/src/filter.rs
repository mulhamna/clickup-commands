//! `TaskFilter` → query string. Filters hold resolved ids; name resolution
//! happens in the CLI / MCP layer via `Resolver`.

use serde::Serialize;
use serde_json::Value;

use crate::{
    client::Query,
    error::{ClickUpError, Result},
};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Range {
    pub gt: Option<i64>,
    pub lt: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldFilter {
    pub field_id: String,
    pub operator: String,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub value: Value,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OrderBy {
    Id,
    Created,
    Updated,
    DueDate,
}

impl std::str::FromStr for OrderBy {
    type Err = ClickUpError;
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "id" => Self::Id,
            "created" => Self::Created,
            "updated" => Self::Updated,
            "due" | "due_date" => Self::DueDate,
            other => {
                return Err(ClickUpError::Invalid(format!(
                    "unknown order '{other}' (use created, updated, due or id)"
                )))
            }
        })
    }
}

impl OrderBy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::Created => "created",
            Self::Updated => "updated",
            Self::DueDate => "due_date",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    pub space_ids: Vec<String>,
    pub folder_ids: Vec<String>,
    pub list_ids: Vec<String>,
    pub statuses: Vec<String>,
    pub include_closed: bool,
    pub assignees: Vec<String>,
    pub tags: Vec<String>,
    pub due: Range,
    pub created: Range,
    pub updated: Range,
    pub done: Range,
    pub custom_fields: Vec<FieldFilter>,
    pub task_types: Vec<i64>,
    pub parent: Option<String>,
    pub subtasks: bool,
    pub archived: Option<bool>,
    pub order_by: Option<OrderBy>,
    pub reverse: bool,
    pub include_markdown: bool,
    pub limit: Option<usize>,
}

impl TaskFilter {
    /// `Some(list_id)` when the filter targets exactly one list and nothing
    /// broader — then the cheaper `GET /v2/list/{id}/task` is used.
    pub fn single_list(&self) -> Option<&str> {
        match (
            self.list_ids.as_slice(),
            self.space_ids.is_empty() && self.folder_ids.is_empty(),
        ) {
            ([one], true) => Some(one),
            _ => None,
        }
    }

    pub fn query(&self) -> Result<Query> {
        let single = self.single_list().is_some();
        let mut q = Query::new();
        if !single {
            q = q
                .push_all("space_ids[]", &self.space_ids)
                .push_all("project_ids[]", &self.folder_ids)
                .push_all("list_ids[]", &self.list_ids);
        }
        q = q
            .push_all("statuses[]", &self.statuses)
            .push_if("include_closed", self.include_closed)
            .push_all("assignees[]", &self.assignees)
            .push_all("tags[]", &self.tags);
        for (prefix, r) in [
            ("due_date", self.due),
            ("date_created", self.created),
            ("date_updated", self.updated),
            ("date_done", self.done),
        ] {
            q = q
                .push_opt(&format!("{prefix}_gt"), r.gt)
                .push_opt(&format!("{prefix}_lt"), r.lt);
        }
        if !self.custom_fields.is_empty() {
            q = q.push("custom_fields", serde_json::to_string(&self.custom_fields)?);
        }
        q = q
            .push_all("custom_items[]", &self.task_types)
            .push_opt("parent", self.parent.as_ref())
            .push_if("subtasks", self.subtasks);
        if single {
            q = q.push_opt("archived", self.archived);
        }
        q = q
            .push_opt("order_by", self.order_by.map(OrderBy::as_str))
            .push_if("reverse", self.reverse)
            .push_if("include_markdown_description", self.include_markdown);
        Ok(q)
    }
}

/// Operators accepted in `--field 'Name<op>value'`, longest first so `<=`
/// wins over `<`.
const SYMBOL_OPS: [&str; 6] = ["!=", "<=", ">=", "=", "<", ">"];
const WORD_OPS: [&str; 7] = [
    "IS NOT NULL",
    "IS NULL",
    "NOT ANY",
    "NOT ALL",
    "RANGE",
    "ANY",
    "ALL",
];

/// Split `Severity=High`, `Points >= 3`, `Due RANGE a,b`, `Owner IS NULL`
/// into (field name, operator, raw value).
pub fn parse_field_expr(expr: &str) -> Result<(String, String, Option<String>)> {
    let invalid = || {
        ClickUpError::Invalid(format!(
            "cannot parse field filter '{expr}' (try 'Severity=High', 'Points>=3', 'Owner IS NULL')"
        ))
    };
    let upper = expr.to_ascii_uppercase();
    for op in WORD_OPS {
        let needle = format!(" {op}");
        if let Some(pos) = upper.find(&needle) {
            let after = pos + needle.len();
            if after < expr.len() && !expr[after..].starts_with(' ') {
                continue;
            }
            let name = expr[..pos].trim();
            let value = expr[after..].trim();
            if name.is_empty() {
                return Err(invalid());
            }
            let needs_value = !op.contains("NULL");
            if needs_value == value.is_empty() {
                return Err(invalid());
            }
            return Ok((
                name.to_string(),
                op.to_string(),
                needs_value.then(|| value.to_string()),
            ));
        }
    }
    let (pos, op) = SYMBOL_OPS
        .iter()
        .filter_map(|op| expr.find(op).map(|p| (p, *op)))
        .min_by_key(|(p, op)| (*p, std::cmp::Reverse(op.len())))
        .ok_or_else(invalid)?;
    let name = expr[..pos].trim();
    let value = expr[pos + op.len()..].trim();
    if name.is_empty() || value.is_empty() {
        return Err(invalid());
    }
    Ok((name.to_string(), op.to_string(), Some(value.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_list_uses_list_endpoint_params() {
        let f = TaskFilter {
            list_ids: vec!["123".into()],
            statuses: vec!["in progress".into()],
            archived: Some(false),
            ..Default::default()
        };
        assert_eq!(f.single_list(), Some("123"));
        let q = f.query().unwrap();
        assert!(q.get("list_ids[]").is_none());
        assert_eq!(q.get("statuses[]"), Some("in progress"));
        assert_eq!(q.get("archived"), Some("false"));
    }

    #[test]
    fn team_query() {
        let f = TaskFilter {
            space_ids: vec!["s1".into()],
            list_ids: vec!["l1".into(), "l2".into()],
            folder_ids: vec!["f1".into()],
            include_closed: true,
            assignees: vec!["7".into()],
            tags: vec!["bug".into()],
            due: Range {
                gt: Some(1),
                lt: Some(2),
            },
            done: Range {
                gt: None,
                lt: Some(9),
            },
            custom_fields: vec![FieldFilter {
                field_id: "fid".into(),
                operator: "=".into(),
                value: Value::String("opt".into()),
            }],
            task_types: vec![1],
            order_by: Some(OrderBy::DueDate),
            reverse: true,
            archived: Some(true),
            ..Default::default()
        };
        assert_eq!(f.single_list(), None);
        let q = f.query().unwrap();
        let pairs: Vec<(&str, &str)> = q
            .pairs()
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("space_ids[]", "s1"),
                ("project_ids[]", "f1"),
                ("list_ids[]", "l1"),
                ("list_ids[]", "l2"),
                ("include_closed", "true"),
                ("assignees[]", "7"),
                ("tags[]", "bug"),
                ("due_date_gt", "1"),
                ("due_date_lt", "2"),
                ("date_done_lt", "9"),
                (
                    "custom_fields",
                    r#"[{"field_id":"fid","operator":"=","value":"opt"}]"#
                ),
                ("custom_items[]", "1"),
                ("order_by", "due_date"),
                ("reverse", "true"),
            ]
        );
    }

    #[test]
    fn field_exprs() {
        let p = |s| parse_field_expr(s).unwrap();
        assert_eq!(
            p("Severity=High"),
            ("Severity".into(), "=".into(), Some("High".into()))
        );
        assert_eq!(
            p("Points >= 3"),
            ("Points".into(), ">=".into(), Some("3".into()))
        );
        assert_eq!(p("A!=b"), ("A".into(), "!=".into(), Some("b".into())));
        assert_eq!(p("Owner IS NULL"), ("Owner".into(), "IS NULL".into(), None));
        assert_eq!(
            p("Owner is not null"),
            ("Owner".into(), "IS NOT NULL".into(), None)
        );
        assert_eq!(
            p("Due RANGE 1,2"),
            ("Due".into(), "RANGE".into(), Some("1,2".into()))
        );
        assert_eq!(
            p("Tags NOT ANY a,b"),
            ("Tags".into(), "NOT ANY".into(), Some("a,b".into()))
        );
        assert!(parse_field_expr("novalue").is_err());
        assert!(parse_field_expr("=x").is_err());
        assert!(parse_field_expr("A IS NULL x").is_err());
    }
}
