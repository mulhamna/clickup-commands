//! Tool inputs. Doc comments become the JSON schema descriptions the model
//! sees, so they are written for the model.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn any_json_object(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({ "type": "object", "additionalProperties": true })
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolResponse {
    #[schemars(schema_with = "any_json_object")]
    pub result: Value,
}

/// Which ClickUp account / workspace a call runs against. Both optional.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct Account {
    /// clickupc profile (account) for this call; see clickup_profile_list. Default: the server's profile.
    pub profile: Option<String>,
    /// Workspace name or id for this call. Default: the profile's workspace.
    pub workspace: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct HierarchyArgs {
    #[serde(flatten)]
    pub account: Account,
    /// Only this space (name or id).
    pub space: Option<String>,
    /// Bypass the 1-hour cache.
    pub refresh: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ListArgs {
    #[serde(flatten)]
    pub account: Account,
    /// List id, alias, "Space/Folder/List" path, or unique list name.
    pub list: String,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct MemberListArgs {
    #[serde(flatten)]
    pub account: Account,
    /// Limit to members of this list; omit for the whole workspace.
    pub list: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MemberResolveArgs {
    #[serde(flatten)]
    pub account: Account,
    /// Names, emails, user ids, or "me".
    pub queries: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ApiRequestArgs {
    #[serde(flatten)]
    pub account: Account,
    /// GET, POST, PUT, PATCH or DELETE.
    pub method: String,
    /// Path starting with /v2/ or /v3/; "{workspace}" is replaced with the workspace id.
    pub path: String,
    /// Query parameters (string, number or boolean values).
    pub query: Option<BTreeMap<String, Value>>,
    /// JSON body for POST/PUT/PATCH.
    pub body: Option<Value>,
    /// Required (true) for anything other than GET.
    pub confirm: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FieldFilterArg {
    /// Custom field name or id.
    pub field: String,
    /// =, ==, !=, !==, <, <=, >, >=, IS NULL, IS NOT NULL, RANGE, ANY, ALL, NOT ANY, NOT ALL.
    pub op: String,
    /// Human value (option name, date, number). For RANGE/ANY/ALL use comma-separated values. Omit for IS (NOT) NULL.
    pub value: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct TaskSearchArgs {
    #[serde(flatten)]
    pub account: Account,
    /// Lists (names, aliases, paths or ids). Exactly one list enables status validation and the cheaper list endpoint.
    #[serde(default)]
    pub lists: Vec<String>,
    #[serde(default)]
    pub folders: Vec<String>,
    #[serde(default)]
    pub spaces: Vec<String>,
    /// Status names. Closed statuses also need include_closed=true.
    #[serde(default)]
    pub statuses: Vec<String>,
    pub include_closed: Option<bool>,
    /// Members: "me", username, email or id.
    #[serde(default)]
    pub assignees: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Dates accept 2026-10-20, "2026-10-20 14:00", today, tomorrow, fri, +3d, -2w, eow.
    pub due_before: Option<String>,
    pub due_after: Option<String>,
    /// Only tasks past their due date.
    pub overdue: Option<bool>,
    pub created_before: Option<String>,
    pub created_after: Option<String>,
    pub updated_before: Option<String>,
    pub updated_after: Option<String>,
    pub done_before: Option<String>,
    pub done_after: Option<String>,
    /// Custom field filters; field names need at least one list in `lists`.
    #[serde(default)]
    pub custom_fields: Vec<FieldFilterArg>,
    /// "task", "milestone", a custom task type name, or id.
    #[serde(default)]
    pub task_types: Vec<String>,
    /// Return subtasks of this task.
    pub parent: Option<String>,
    /// Include subtasks in results.
    pub subtasks: Option<bool>,
    /// created (default), updated, due, id.
    pub order_by: Option<String>,
    pub reverse: Option<bool>,
    /// Default 50, max 500.
    pub limit: Option<usize>,
    /// "summary" (default) or "full".
    pub detail: Option<String>,
}

/// Task fields shared by create / bulk create.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct TaskFields {
    pub name: String,
    /// Markdown.
    pub description: Option<String>,
    /// Status name valid for the list.
    pub status: Option<String>,
    /// urgent | high | normal | low
    pub priority: Option<String>,
    /// Members: "me", username, email or id.
    #[serde(default)]
    pub assignees: Vec<String>,
    /// Tags must already exist in the space.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Date input (2026-10-20, tomorrow, +3d, eow …).
    pub due: Option<String>,
    pub start: Option<String>,
    /// Duration like "2h" or "1h30m".
    pub estimate: Option<String>,
    pub points: Option<f64>,
    /// Parent task (id, custom id or URL) to create a subtask in the same list.
    pub parent: Option<String>,
    /// "task", "milestone" or a custom task type name.
    pub task_type: Option<String>,
    /// Custom field name or id → human value (option name, date, number, member).
    #[serde(default)]
    pub custom_fields: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TaskCreateArgs {
    #[serde(flatten)]
    pub account: Account,
    /// Target list (name, alias, path or id).
    pub list: String,
    #[serde(flatten)]
    pub fields: TaskFields,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TaskGetArgs {
    #[serde(flatten)]
    pub account: Account,
    /// Task id, custom id (DEV-123) or task URL.
    pub task: String,
    /// "summary" (default) or "full".
    pub detail: Option<String>,
    pub include_subtasks: Option<bool>,
    /// Also return the newest 25 comments.
    pub include_comments: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct TaskUpdateArgs {
    #[serde(flatten)]
    pub account: Account,
    /// Task id, custom id (DEV-123) or task URL.
    pub task: String,
    pub name: Option<String>,
    /// Markdown; replaces the description.
    pub description: Option<String>,
    pub status: Option<String>,
    /// urgent | high | normal | low | none
    pub priority: Option<String>,
    /// Date input, or "none" to clear.
    pub due: Option<String>,
    pub start: Option<String>,
    /// Duration, or "none" to clear.
    pub estimate: Option<String>,
    pub points: Option<f64>,
    pub parent: Option<String>,
    pub task_type: Option<String>,
    #[serde(default)]
    pub assignees_add: Vec<String>,
    #[serde(default)]
    pub assignees_remove: Vec<String>,
    #[serde(default)]
    pub watchers_add: Vec<String>,
    #[serde(default)]
    pub watchers_remove: Vec<String>,
    pub archived: Option<bool>,
    /// Custom field name or id → human value; applied after the update via Set Custom Field.
    #[serde(default)]
    pub custom_fields: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TaskStatusArgs {
    #[serde(flatten)]
    pub account: Account,
    pub task: String,
    /// Status name; validated against the task's list.
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TaskMoveArgs {
    #[serde(flatten)]
    pub account: Account,
    pub task: String,
    /// Destination list.
    pub list: String,
    /// Old status name → new status name, needed when the current status doesn't exist in the target list.
    pub status_map: Option<BTreeMap<String, String>>,
    pub move_custom_fields: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TaskConfirmArgs {
    #[serde(flatten)]
    pub account: Account,
    pub task: String,
    /// Must be true.
    pub confirm: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct BulkCreateArgs {
    #[serde(flatten)]
    pub account: Account,
    pub list: String,
    pub tasks: Vec<TaskFields>,
    /// Return the plan without creating anything.
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct BulkChangesArg {
    /// Status name (sent as given; per-task errors are reported).
    pub status: Option<String>,
    pub priority: Option<String>,
    #[serde(default)]
    pub assignees_add: Vec<String>,
    #[serde(default)]
    pub tags_add: Vec<String>,
    /// Date input, or "none" to clear.
    pub due: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct BulkUpdateArgs {
    #[serde(flatten)]
    pub account: Account,
    /// Explicit tasks (ids, custom ids or URLs). Use this or `filter`.
    pub tasks: Option<Vec<String>>,
    /// Same shape as clickup_task_search input.
    pub filter: Option<TaskSearchArgs>,
    pub changes: BulkChangesArg,
    pub dry_run: Option<bool>,
    /// Required when more than 1 task would change.
    pub confirm: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct BatchArgs {
    #[serde(flatten)]
    pub account: Account,
    /// [{op: "create", list, name, …} | {op: "update", task, assign, status, …} | {op: "status", task, status} | {op: "comment", task, text} | {op: "delete", task}]
    pub ops: Vec<Value>,
    pub dry_run: Option<bool>,
    /// Required when the batch has more than 1 op or any delete.
    pub confirm: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CommentListArgs {
    #[serde(flatten)]
    pub account: Account,
    pub task: String,
    /// Default 25.
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CommentAddArgs {
    #[serde(flatten)]
    pub account: Account,
    pub task: String,
    pub text: String,
    /// Assign the comment to this member.
    pub assignee: Option<String>,
    pub notify_all: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CommentUpdateArgs {
    #[serde(flatten)]
    pub account: Account,
    pub comment_id: String,
    pub text: String,
    pub resolved: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CommentDeleteArgs {
    #[serde(flatten)]
    pub account: Account,
    pub comment_id: String,
    /// Must be true.
    pub confirm: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CommentIdArgs {
    #[serde(flatten)]
    pub account: Account,
    pub comment_id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CommentReplyArgs {
    #[serde(flatten)]
    pub account: Account,
    pub comment_id: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FieldSetArgs {
    #[serde(flatten)]
    pub account: Account,
    pub task: String,
    /// Field name or id.
    pub field: String,
    /// Human value: option name, comma-separated label names, date, number, true/false, member(s). Prefix a member/task with "-" to remove it.
    pub value: Value,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FieldClearArgs {
    #[serde(flatten)]
    pub account: Account,
    pub task: String,
    pub field: String,
    /// Must be true.
    pub confirm: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TagArgs {
    #[serde(flatten)]
    pub account: Account,
    pub task: String,
    /// Tag names (must exist in the space).
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LinkArgs {
    #[serde(flatten)]
    pub account: Account,
    pub task: String,
    pub other_task: String,
    /// Required (true) when removing.
    pub confirm: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DependencyArgs {
    #[serde(flatten)]
    pub account: Account,
    pub task: String,
    /// `task` blocks this task.
    pub blocks: Option<String>,
    /// `task` is waiting on this task.
    pub waiting_on: Option<String>,
    /// Required (true) when removing.
    pub confirm: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ChecklistAddArgs {
    #[serde(flatten)]
    pub account: Account,
    pub task: String,
    pub name: String,
    #[serde(default)]
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ChecklistItemUpdateArgs {
    #[serde(flatten)]
    pub account: Account,
    pub checklist_id: String,
    pub item_id: String,
    pub resolved: Option<bool>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AttachmentArgs {
    #[serde(flatten)]
    pub account: Account,
    pub task: String,
    /// Local file path on the machine running the server.
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TaskListArgs {
    #[serde(flatten)]
    pub account: Account,
    pub task: String,
    pub list: String,
    /// Required (true) when removing.
    pub confirm: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct TimeListArgs {
    #[serde(flatten)]
    pub account: Account,
    pub task: Option<String>,
    pub list: Option<String>,
    pub space: Option<String>,
    /// Date input; default is the last 30 days.
    pub from: Option<String>,
    pub to: Option<String>,
    /// Members (owner/admin only for other users).
    #[serde(default)]
    pub users: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TimeAddArgs {
    #[serde(flatten)]
    pub account: Account,
    pub task: String,
    /// Duration like "1h30m".
    pub duration: String,
    /// Start time; default now minus duration.
    pub at: Option<String>,
    pub note: Option<String>,
    pub billable: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct TimeStartArgs {
    #[serde(flatten)]
    pub account: Account,
    pub task: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TimeDeleteArgs {
    #[serde(flatten)]
    pub account: Account,
    pub entry_id: String,
    /// Must be true.
    pub confirm: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct TimeInStatusArgs {
    #[serde(flatten)]
    pub account: Account,
    pub task: Option<String>,
    /// Several plain task ids.
    pub tasks: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct StandupArgs {
    #[serde(flatten)]
    pub account: Account,
    /// Default "yesterday".
    pub since: Option<String>,
    /// Default "me".
    pub user: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ListCreateArgs {
    #[serde(flatten)]
    pub account: Account,
    pub name: String,
    /// Folder to create in; or use `space` for a folderless list.
    pub folder: Option<String>,
    pub space: Option<String>,
    /// Markdown description.
    pub content: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FolderCreateArgs {
    #[serde(flatten)]
    pub account: Account,
    pub name: String,
    pub space: String,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct DocSearchArgs {
    #[serde(flatten)]
    pub account: Account,
    /// Case-insensitive substring of the doc name (filtered client-side).
    pub query: Option<String>,
    /// Default 50.
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DocGetArgs {
    #[serde(flatten)]
    pub account: Account,
    pub doc_id: String,
    /// Return this page's markdown content.
    pub page_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DocIdArgs {
    #[serde(flatten)]
    pub account: Account,
    pub doc_id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DocCreateArgs {
    #[serde(flatten)]
    pub account: Account,
    pub name: String,
    /// space | folder | list | workspace (default workspace).
    pub parent_type: Option<String>,
    /// Name or id of the parent space / folder / list.
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DocPageCreateArgs {
    #[serde(flatten)]
    pub account: Account,
    pub doc_id: String,
    pub name: String,
    /// Markdown.
    pub content: Option<String>,
    pub parent_page_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DocPageUpdateArgs {
    #[serde(flatten)]
    pub account: Account,
    pub doc_id: String,
    pub page_id: String,
    /// Markdown.
    pub content: Option<String>,
    pub name: Option<String>,
    /// replace (default) | append | prepend
    pub mode: Option<String>,
}
