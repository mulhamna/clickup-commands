use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::{
        stdio,
        streamable_http_server::{
            session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
        },
    },
    ErrorData, Json, ServerHandler, ServiceExt,
};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::{
    app,
    error::AppResult,
    models::{
        ApiRequestArgs, AttachmentArgs, BatchArgs, BulkCreateArgs, BulkUpdateArgs,
        ChecklistAddArgs, ChecklistItemUpdateArgs, CommentAddArgs, CommentDeleteArgs,
        CommentIdArgs, CommentListArgs, CommentReplyArgs, CommentUpdateArgs, DependencyArgs,
        DocCreateArgs, DocGetArgs, DocIdArgs, DocPageCreateArgs, DocPageUpdateArgs, DocSearchArgs,
        FieldClearArgs, FieldSetArgs, FolderCreateArgs, HierarchyArgs, LinkArgs, ListArgs,
        ListCreateArgs, MemberListArgs, MemberResolveArgs, StandupArgs, TagArgs, TaskConfirmArgs,
        TaskCreateArgs, TaskGetArgs, TaskListArgs, TaskMoveArgs, TaskSearchArgs, TaskStatusArgs,
        TaskUpdateArgs, TimeAddArgs, TimeDeleteArgs, TimeInStatusArgs, TimeListArgs, TimeStartArgs,
        ToolResponse,
    },
};

#[derive(Clone)]
pub struct ClickUpMcpServer {
    tool_router: ToolRouter<Self>,
}

impl ClickUpMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

impl Default for ClickUpMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

/// MCP stdio frames are single JSON lines and several clients cap line
/// length (e.g. 64 KB); keep responses comfortably below that.
const MAX_RESPONSE_BYTES: usize = 48 * 1024;
const MAX_STRING_CHARS: usize = 2000;

fn bound_response(mut value: Value) -> Value {
    if serde_json::to_string(&value).map_or(0, |s| s.len()) <= MAX_RESPONSE_BYTES {
        return value;
    }
    truncate_long_strings(&mut value, MAX_STRING_CHARS);
    if let Value::Object(map) = &mut value {
        map.insert("_truncated".into(), Value::Bool(true));
    }
    value
}

fn truncate_long_strings(value: &mut Value, max: usize) {
    match value {
        Value::String(text) if text.chars().count() > max => {
            let kept: String = text.chars().take(max).collect();
            *text = format!("{kept}…[truncated]");
        }
        Value::Array(items) => items.iter_mut().for_each(|v| truncate_long_strings(v, max)),
        Value::Object(map) => map.values_mut().for_each(|v| truncate_long_strings(v, max)),
        _ => {}
    }
}

fn respond(result: AppResult<Value>) -> Result<Json<ToolResponse>, ErrorData> {
    result
        .map(|v| {
            Json(ToolResponse {
                result: bound_response(v),
            })
        })
        .map_err(|e| e.to_mcp())
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for ClickUpMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("clickupc", env!("CARGO_PKG_VERSION"))
                    .with_title("clickupc MCP")
                    .with_description("Unofficial ClickUp tools for MCP clients"),
            )
            .with_instructions(
                "Use the clickup_* tools for ClickUp tasks, comments, custom fields, time tracking, docs and raw API access. Inputs accept names (lists, statuses, members, fields, options) or ids, and dates like 'tomorrow' or '2026-10-20'. Delete/remove/clear tools require confirm=true. Use clickup_field_set to change custom fields.",
            )
    }
}

#[tool_router(router = tool_router)]
impl ClickUpMcpServer {
    #[tool(
        name = "clickup_auth_status",
        description = "Show the active profile, workspace id, whether a token is configured (masked), and the config path. No API call. Use first when other tools return auth_missing."
    )]
    pub async fn clickup_auth_status(&self) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::auth_status())
    }

    #[tool(
        name = "clickup_whoami",
        description = "Show the authenticated ClickUp user (id, username, email, timezone) plus the current workspace and plan."
    )]
    pub async fn clickup_whoami(&self) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::whoami().await)
    }

    #[tool(
        name = "clickup_workspace_list",
        description = "List workspaces the token can access and which one is current. Workspace = 'team' in ClickUp API v2."
    )]
    pub async fn clickup_workspace_list(&self) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::workspace_list().await)
    }

    #[tool(
        name = "clickup_hierarchy",
        description = "Space → Folder → List tree with ids and names (cached for 1 hour; refresh=true to bypass). Use it to discover list names before searching or creating tasks."
    )]
    pub async fn clickup_hierarchy(
        &self,
        Parameters(args): Parameters<HierarchyArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::hierarchy(args).await)
    }

    #[tool(
        name = "clickup_list_get",
        description = "Details for one list: statuses (name, type, order), custom fields (name, type, options) and members. Call before creating/updating tasks to learn valid statuses and field names."
    )]
    pub async fn clickup_list_get(
        &self,
        Parameters(args): Parameters<ListArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::list_get(args).await)
    }

    #[tool(
        name = "clickup_member_list",
        description = "List workspace members, or members of one list."
    )]
    pub async fn clickup_member_list(
        &self,
        Parameters(args): Parameters<MemberListArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::member_list(args).await)
    }

    #[tool(
        name = "clickup_member_resolve",
        description = "Resolve names, emails or 'me' to ClickUp user ids. Unresolved queries return resolve_error data with suggestions."
    )]
    pub async fn clickup_member_resolve(
        &self,
        Parameters(args): Parameters<MemberResolveArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::member_resolve(args).await)
    }

    #[tool(
        name = "clickup_api_request",
        description = "Raw ClickUp API call for endpoints without a dedicated tool. Path must start with /v2/ or /v3/; '{workspace}' is substituted. Non-GET requires confirm=true."
    )]
    pub async fn clickup_api_request(
        &self,
        Parameters(args): Parameters<ApiRequestArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::api_request(args).await)
    }

    #[tool(
        name = "clickup_task_search",
        description = "Search tasks by list/folder/space, status, assignee, tag, dates, custom fields and task type. Names are resolved for you. Returns compact summaries (detail='full' for everything). Closed tasks need include_closed=true."
    )]
    pub async fn clickup_task_search(
        &self,
        Parameters(args): Parameters<TaskSearchArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::task_search(args).await)
    }

    #[tool(
        name = "clickup_task_get",
        description = "Get one task by id, custom id (DEV-123) or URL. detail='full' adds markdown description, custom fields, checklists and relations; include_comments adds the newest 25 comments."
    )]
    pub async fn clickup_task_get(
        &self,
        Parameters(args): Parameters<TaskGetArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::task_get(args).await)
    }

    #[tool(
        name = "clickup_task_create",
        description = "Create a task in a list. Accepts names for status, assignees, task type and custom fields; dates like 'tomorrow' or '2026-10-20'; estimate like '2h'. Description is markdown."
    )]
    pub async fn clickup_task_create(
        &self,
        Parameters(args): Parameters<TaskCreateArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::task_create(args).await)
    }

    #[tool(
        name = "clickup_task_update",
        description = "Update task fields. Use assignees_add/assignees_remove (not a full list). custom_fields are applied afterwards via Set Custom Field and reported per field; use clickup_field_set for a single field. Set due/estimate to 'none' to clear."
    )]
    pub async fn clickup_task_update(
        &self,
        Parameters(args): Parameters<TaskUpdateArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::task_update(args).await)
    }

    #[tool(
        name = "clickup_task_set_status",
        description = "Change a task's status; validated against the task's list (case-insensitive). Mismatches return resolve_error with suggestions."
    )]
    pub async fn clickup_task_set_status(
        &self,
        Parameters(args): Parameters<TaskStatusArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::task_set_status(args).await)
    }

    #[tool(
        name = "clickup_task_move",
        description = "Move a task to a new home list. If the current status doesn't exist in the target list, pass status_map {old: new}; otherwise the error lists the target list's statuses."
    )]
    pub async fn clickup_task_move(
        &self,
        Parameters(args): Parameters<TaskMoveArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::task_move(args).await)
    }

    #[tool(
        name = "clickup_task_delete",
        description = "Permanently delete a task. Requires confirm=true."
    )]
    pub async fn clickup_task_delete(
        &self,
        Parameters(args): Parameters<TaskConfirmArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::task_delete(args).await)
    }

    #[tool(
        name = "clickup_task_bulk_create",
        description = "Create many tasks in one list with bounded concurrency and per-item results. dry_run=true returns the plan only."
    )]
    pub async fn clickup_task_bulk_create(
        &self,
        Parameters(args): Parameters<BulkCreateArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::task_bulk_create(args).await)
    }

    #[tool(
        name = "clickup_task_bulk_update",
        description = "Apply the same changes (status, priority, assignees_add, tags_add, due) to explicit tasks or to every task matching a search filter. dry_run=true lists matches; more than 1 task requires confirm=true."
    )]
    pub async fn clickup_task_bulk_update(
        &self,
        Parameters(args): Parameters<BulkUpdateArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::task_bulk_update(args).await)
    }

    #[tool(
        name = "clickup_task_batch",
        description = "Run mixed operations in sequence: create, update, status, comment, delete. dry_run=true returns the plan; more than 1 op or any delete requires confirm=true. Per-op results are returned."
    )]
    pub async fn clickup_task_batch(
        &self,
        Parameters(args): Parameters<BatchArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::task_batch(args).await)
    }

    #[tool(
        name = "clickup_comment_list",
        description = "List task comments, newest first (default 25; pagination handled)."
    )]
    pub async fn clickup_comment_list(
        &self,
        Parameters(args): Parameters<CommentListArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::comment_list(args).await)
    }

    #[tool(
        name = "clickup_comment_add",
        description = "Add a plain-text comment to a task, optionally assigned to a member."
    )]
    pub async fn clickup_comment_add(
        &self,
        Parameters(args): Parameters<CommentAddArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::comment_add(args).await)
    }

    #[tool(
        name = "clickup_comment_update",
        description = "Edit a comment's text and/or mark it resolved."
    )]
    pub async fn clickup_comment_update(
        &self,
        Parameters(args): Parameters<CommentUpdateArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::comment_update(args).await)
    }

    #[tool(
        name = "clickup_comment_delete",
        description = "Delete a comment. Requires confirm=true."
    )]
    pub async fn clickup_comment_delete(
        &self,
        Parameters(args): Parameters<CommentDeleteArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::comment_delete(args).await)
    }

    #[tool(
        name = "clickup_comment_replies",
        description = "List threaded replies to a comment."
    )]
    pub async fn clickup_comment_replies(
        &self,
        Parameters(args): Parameters<CommentIdArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::comment_replies(args).await)
    }

    #[tool(
        name = "clickup_comment_reply",
        description = "Reply in a comment thread."
    )]
    pub async fn clickup_comment_reply(
        &self,
        Parameters(args): Parameters<CommentReplyArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::comment_reply(args).await)
    }

    #[tool(
        name = "clickup_field_list",
        description = "Custom fields available in a list: name, id, type, options."
    )]
    pub async fn clickup_field_list(
        &self,
        Parameters(args): Parameters<ListArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::field_list(args).await)
    }

    #[tool(
        name = "clickup_field_set",
        description = "Set one custom field on a task from a human value (option name, labels, date, number, checkbox, members). This is the only way to change custom fields; clickup_task_update applies them through this same call."
    )]
    pub async fn clickup_field_set(
        &self,
        Parameters(args): Parameters<FieldSetArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::field_set(args).await)
    }

    #[tool(
        name = "clickup_field_clear",
        description = "Remove a custom field's value from a task. Requires confirm=true."
    )]
    pub async fn clickup_field_clear(
        &self,
        Parameters(args): Parameters<FieldClearArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::field_clear(args).await)
    }

    #[tool(
        name = "clickup_tag_add",
        description = "Add existing space tags to a task."
    )]
    pub async fn clickup_tag_add(
        &self,
        Parameters(args): Parameters<TagArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::tag_add(args).await)
    }

    #[tool(name = "clickup_tag_remove", description = "Remove tags from a task.")]
    pub async fn clickup_tag_remove(
        &self,
        Parameters(args): Parameters<TagArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::tag_remove(args).await)
    }

    #[tool(name = "clickup_link_add", description = "Link two tasks.")]
    pub async fn clickup_link_add(
        &self,
        Parameters(args): Parameters<LinkArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::link_add(args).await)
    }

    #[tool(
        name = "clickup_link_remove",
        description = "Remove a link between two tasks. Requires confirm=true."
    )]
    pub async fn clickup_link_remove(
        &self,
        Parameters(args): Parameters<LinkArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::link_remove(args).await)
    }

    #[tool(
        name = "clickup_dependency_add",
        description = "Add a dependency: `blocks` (this task blocks the other) or `waiting_on` (this task waits on the other)."
    )]
    pub async fn clickup_dependency_add(
        &self,
        Parameters(args): Parameters<DependencyArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::dependency_add(args).await)
    }

    #[tool(
        name = "clickup_dependency_remove",
        description = "Remove a dependency. Requires confirm=true."
    )]
    pub async fn clickup_dependency_remove(
        &self,
        Parameters(args): Parameters<DependencyArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::dependency_remove(args).await)
    }

    #[tool(
        name = "clickup_checklist_add",
        description = "Create a checklist on a task, optionally with items."
    )]
    pub async fn clickup_checklist_add(
        &self,
        Parameters(args): Parameters<ChecklistAddArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::checklist_add(args).await)
    }

    #[tool(
        name = "clickup_checklist_item_update",
        description = "Resolve/unresolve or rename a checklist item."
    )]
    pub async fn clickup_checklist_item_update(
        &self,
        Parameters(args): Parameters<ChecklistItemUpdateArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::checklist_item_update(args).await)
    }

    #[tool(
        name = "clickup_attachment_upload",
        description = "Upload a local file as a task attachment."
    )]
    pub async fn clickup_attachment_upload(
        &self,
        Parameters(args): Parameters<AttachmentArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::attachment_upload(args).await)
    }

    #[tool(
        name = "clickup_task_add_to_list",
        description = "Add a task to an additional list (Tasks in Multiple Lists); its home list is unchanged."
    )]
    pub async fn clickup_task_add_to_list(
        &self,
        Parameters(args): Parameters<TaskListArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::task_add_to_list(args).await)
    }

    #[tool(
        name = "clickup_task_remove_from_list",
        description = "Remove a task from an additional list. Requires confirm=true."
    )]
    pub async fn clickup_task_remove_from_list(
        &self,
        Parameters(args): Parameters<TaskListArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::task_remove_from_list(args).await)
    }

    #[tool(
        name = "clickup_time_list",
        description = "List time entries (default: last 30 days, current user) with durations like '1h 30m' and a total."
    )]
    pub async fn clickup_time_list(
        &self,
        Parameters(args): Parameters<TimeListArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::time_list(args).await)
    }

    #[tool(
        name = "clickup_time_add",
        description = "Log time on a task, e.g. duration '1h30m'. Default start is now minus the duration."
    )]
    pub async fn clickup_time_add(
        &self,
        Parameters(args): Parameters<TimeAddArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::time_add(args).await)
    }

    #[tool(
        name = "clickup_time_start",
        description = "Start a timer, optionally on a task."
    )]
    pub async fn clickup_time_start(
        &self,
        Parameters(args): Parameters<TimeStartArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::time_start(args).await)
    }

    #[tool(name = "clickup_time_stop", description = "Stop the running timer.")]
    pub async fn clickup_time_stop(&self) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::time_stop().await)
    }

    #[tool(
        name = "clickup_time_current",
        description = "The running time entry, or null."
    )]
    pub async fn clickup_time_current(&self) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::time_current().await)
    }

    #[tool(
        name = "clickup_time_delete",
        description = "Delete a time entry. Requires confirm=true."
    )]
    pub async fn clickup_time_delete(
        &self,
        Parameters(args): Parameters<TimeDeleteArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::time_delete(args).await)
    }

    #[tool(
        name = "clickup_time_in_status",
        description = "How long a task (or several tasks) spent in each status."
    )]
    pub async fn clickup_time_in_status(
        &self,
        Parameters(args): Parameters<TimeInStatusArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::time_in_status(args).await)
    }

    #[tool(
        name = "clickup_standup",
        description = "Standup summary for a user (default me): tasks closed and updated since (default yesterday), in progress, due within 3 days, overdue."
    )]
    pub async fn clickup_standup(
        &self,
        Parameters(args): Parameters<StandupArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::standup(args).await)
    }

    #[tool(
        name = "clickup_list_summary",
        description = "Counts for a list: total, by status, by assignee, overdue."
    )]
    pub async fn clickup_list_summary(
        &self,
        Parameters(args): Parameters<ListArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::list_summary(args).await)
    }

    #[tool(
        name = "clickup_list_create",
        description = "Create a list in a folder, or a folderless list in a space."
    )]
    pub async fn clickup_list_create(
        &self,
        Parameters(args): Parameters<ListCreateArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::list_create(args).await)
    }

    #[tool(
        name = "clickup_folder_create",
        description = "Create a folder in a space."
    )]
    pub async fn clickup_folder_create(
        &self,
        Parameters(args): Parameters<FolderCreateArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::folder_create(args).await)
    }

    #[tool(
        name = "clickup_doc_search",
        description = "List docs in the workspace, optionally filtered by a name substring."
    )]
    pub async fn clickup_doc_search(
        &self,
        Parameters(args): Parameters<DocSearchArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::doc_search(args).await)
    }

    #[tool(
        name = "clickup_doc_get",
        description = "Get a doc and its page listing, or one page's markdown with page_id."
    )]
    pub async fn clickup_doc_get(
        &self,
        Parameters(args): Parameters<DocGetArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::doc_get(args).await)
    }

    #[tool(
        name = "clickup_doc_pages",
        description = "All pages of a doc with markdown content."
    )]
    pub async fn clickup_doc_pages(
        &self,
        Parameters(args): Parameters<DocIdArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::doc_pages(args).await)
    }

    #[tool(
        name = "clickup_doc_create",
        description = "Create a doc in the workspace or under a space, folder or list."
    )]
    pub async fn clickup_doc_create(
        &self,
        Parameters(args): Parameters<DocCreateArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::doc_create(args).await)
    }

    #[tool(
        name = "clickup_doc_page_create",
        description = "Add a markdown page to a doc."
    )]
    pub async fn clickup_doc_page_create(
        &self,
        Parameters(args): Parameters<DocPageCreateArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::doc_page_create(args).await)
    }

    #[tool(
        name = "clickup_doc_page_update",
        description = "Edit a doc page: replace, append or prepend markdown content, or rename it."
    )]
    pub async fn clickup_doc_page_update(
        &self,
        Parameters(args): Parameters<DocPageUpdateArgs>,
    ) -> Result<Json<ToolResponse>, ErrorData> {
        respond(app::doc_page_update(args).await)
    }
}

pub async fn run_stdio() -> anyhow::Result<()> {
    let server = ClickUpMcpServer::new().serve(stdio()).await?;
    server.waiting().await?;
    Ok(())
}

pub async fn run_streamable_http(host: &str, port: u16, path: &str) -> anyhow::Result<()> {
    let cancellation_token = CancellationToken::new();
    let allowed_hosts = [
        format!("{host}:{port}"),
        host.to_string(),
        format!("127.0.0.1:{port}"),
        "127.0.0.1".to_string(),
        format!("localhost:{port}"),
        "localhost".to_string(),
    ];
    let service = StreamableHttpService::new(
        || Ok(ClickUpMcpServer::new()),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_cancellation_token(cancellation_token.child_token())
            .with_allowed_hosts(allowed_hosts),
    );
    let router = axum::Router::new().nest_service(path, service);
    let listener = tokio::net::TcpListener::bind(format!("{host}:{port}")).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            cancellation_token.cancel();
        })
        .await?;
    Ok(())
}
