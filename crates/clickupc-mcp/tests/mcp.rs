//! In-process MCP tests over a duplex pipe and streamable HTTP. ClickUp is
//! mocked with wiremock via CLICKUP_BASE_URL; nothing reaches the real API.

use clickupc_mcp::ClickUpMcpServer;
use rmcp::{
    model::{CallToolRequestParams, ClientCapabilities, ClientInfo, Implementation},
    service::{RoleClient, RunningService},
    transport::{
        streamable_http_server::{
            session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
        },
        StreamableHttpClientTransport,
    },
    ClientHandler, ServiceExt,
};
use serde_json::{json, Map, Value};
use serial_test::serial;
use tempfile::TempDir;
use wiremock::{
    matchers::{method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

#[derive(Default, Clone)]
struct TestClient;
impl ClientHandler for TestClient {}

const VARS: &[&str] = &[
    "CLICKUP_TOKEN",
    "CLICKUP_WORKSPACE_ID",
    "CLICKUP_PROFILE",
    "CLICKUP_BASE_URL",
    "CLICKUP_CONFIG_DIR",
    "CLICKUP_CACHE_DIR",
    "CLICKUP_TIMEZONE",
];

struct Env {
    _dir: TempDir,
}

impl Env {
    fn new(base_url: Option<&str>) -> Self {
        let dir = TempDir::new().unwrap();
        for v in VARS {
            std::env::remove_var(v);
        }
        std::env::set_var("CLICKUP_CONFIG_DIR", dir.path().join("config"));
        std::env::set_var("CLICKUP_CACHE_DIR", dir.path().join("cache"));
        if let Some(url) = base_url {
            std::env::set_var("CLICKUP_TOKEN", "pk_test");
            std::env::set_var("CLICKUP_WORKSPACE_ID", "9001");
            std::env::set_var("CLICKUP_TIMEZONE", "UTC");
            std::env::set_var("CLICKUP_BASE_URL", url);
        }
        Self { _dir: dir }
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        for v in VARS {
            std::env::remove_var(v);
        }
    }
}

type Client = RunningService<RoleClient, TestClient>;

async fn connect() -> (Client, tokio::task::JoinHandle<()>) {
    let (server_io, client_io) = tokio::io::duplex(256 * 1024);
    let server = tokio::spawn(async move {
        let s = ClickUpMcpServer::new().serve(server_io).await.unwrap();
        let _ = s.waiting().await;
    });
    (TestClient.serve(client_io).await.unwrap(), server)
}

fn args(v: Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap()
}

async fn call(client: &Client, name: &'static str, a: Value) -> Result<Value, String> {
    client
        .call_tool(CallToolRequestParams::new(name).with_arguments(args(a)))
        .await
        .map(|r| r.structured_content.unwrap_or(Value::Null))
        .map_err(|e| e.to_string())
}

#[tokio::test]
#[serial]
async fn stdio_lists_all_tools_with_prefix() {
    let _env = Env::new(None);
    let (client, server) = connect().await;
    let tools = client.list_all_tools().await.unwrap();
    assert!(tools.len() >= 50, "only {} tools", tools.len());
    assert!(tools.iter().all(|t| t.name.starts_with("clickup_")));
    for expected in [
        "clickup_auth_status",
        "clickup_task_search",
        "clickup_task_delete",
        "clickup_field_set",
        "clickup_time_current",
        "clickup_doc_page_update",
        "clickup_api_request",
    ] {
        assert!(
            tools.iter().any(|t| t.name == expected),
            "missing {expected}"
        );
    }
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
#[serial]
async fn auth_status_reports_missing_token() {
    let _env = Env::new(None);
    let (client, server) = connect().await;
    let out = call(&client, "clickup_auth_status", json!({}))
        .await
        .unwrap();
    assert_eq!(out["result"]["token_present"], json!(false));
    let err = call(&client, "clickup_whoami", json!({}))
        .await
        .unwrap_err();
    assert!(err.contains("auth_missing"), "{err}");
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
#[serial]
async fn destructive_tools_require_confirm() {
    let mock = MockServer::start().await;
    let _env = Env::new(Some(&mock.uri()));
    let (client, server) = connect().await;
    for (tool, a) in [
        ("clickup_task_delete", json!({ "task": "abc" })),
        ("clickup_comment_delete", json!({ "comment_id": "1" })),
        ("clickup_time_delete", json!({ "entry_id": "1" })),
        (
            "clickup_field_clear",
            json!({ "task": "abc", "field": "F" }),
        ),
        (
            "clickup_api_request",
            json!({ "method": "POST", "path": "/v2/x" }),
        ),
    ] {
        let err = call(&client, tool, a).await.unwrap_err();
        assert!(err.contains("unsafe_operation"), "{tool}: {err}");
    }
    assert!(mock.received_requests().await.unwrap().is_empty());
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
#[serial]
async fn task_search_returns_compact_summaries() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/list/301/task"))
        .and(query_param("page", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tasks": [{
                "id": "abc", "custom_id": "DEV-1", "name": "Fix login",
                "status": { "status": "in progress", "type": "custom" },
                "description": "long text that should not be in the summary",
                "assignees": [{ "id": 7, "username": "ana" }],
                "due_date": "1792454400000",
                "list": { "id": "301", "name": "Sprint" },
                "url": "https://app.clickup.com/t/abc"
            }],
            "last_page": true
        })))
        .expect(1)
        .mount(&mock)
        .await;
    let _env = Env::new(Some(&mock.uri()));
    let (client, server) = connect().await;
    let out = call(&client, "clickup_task_search", json!({ "lists": ["301"] }))
        .await
        .unwrap();
    let task = &out["result"]["tasks"][0];
    assert_eq!(task["custom_id"], json!("DEV-1"));
    assert_eq!(task["status"], json!("in progress"));
    assert_eq!(task["due"]["iso"], json!("2026-10-20T00:00:00+00:00"));
    assert!(task.get("description").is_none());
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
#[serial]
async fn api_errors_carry_stable_codes() {
    let mock = MockServer::start().await;
    Mock::given(path("/v2/task/abc"))
        .respond_with(
            ResponseTemplate::new(400).set_body_json(json!({ "err": "Bad", "ECODE": "ITEM_015" })),
        )
        .mount(&mock)
        .await;
    let _env = Env::new(Some(&mock.uri()));
    let (client, server) = connect().await;
    let err = call(&client, "clickup_task_get", json!({ "task": "abc" }))
        .await
        .unwrap_err();
    assert!(err.contains("clickup_api_error"), "{err}");
    let err = call(&client, "clickup_task_get", json!({ "task": "not a task" }))
        .await
        .unwrap_err();
    assert!(err.contains("validation_error"), "{err}");
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
#[serial]
async fn streamable_http_transport() {
    let _env = Env::new(None);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let service = StreamableHttpService::new(
        || Ok(ClickUpMcpServer::new()),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_allowed_hosts([format!("127.0.0.1:{port}"), "127.0.0.1".into()]),
    );
    let router = axum::Router::new().nest_service("/mcp", service);
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await
            .unwrap();
    });
    let client = ClientInfo::new(
        ClientCapabilities::default(),
        Implementation::new("test", "0.1.0"),
    )
    .serve(StreamableHttpClientTransport::from_uri(format!(
        "http://127.0.0.1:{port}/mcp"
    )))
    .await
    .unwrap();
    let tools = client.list_all_tools().await.unwrap();
    assert!(tools.iter().any(|t| t.name == "clickup_task_search"));
    client.cancel().await.unwrap();
    let _ = tx.send(());
    let _ = server.await;
}

/// Two accounts in one config: `work` (pk_work…, workspace 111) is active,
/// `personal` (pk_home…, workspace 222).
fn two_profiles(env: &Env, base_url: &str) {
    let dir = env._dir.path().join("config");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("config.toml"),
        "current_profile = \"work\"\n\
         [profiles.work]\ntoken = \"pk_work_aaaaaaaa\"\nworkspace_id = \"111\"\n\
         [profiles.personal]\ntoken = \"pk_home_bbbbbbbb\"\nworkspace_id = \"222\"\n",
    )
    .unwrap();
    std::env::remove_var("CLICKUP_TOKEN");
    std::env::remove_var("CLICKUP_WORKSPACE_ID");
    std::env::set_var("CLICKUP_BASE_URL", base_url);
}

#[tokio::test]
#[serial]
async fn profile_param_switches_account_per_call() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/user"))
        .and(wiremock::matchers::header(
            "authorization",
            "pk_home_bbbbbbbb",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "user": { "id": 2, "username": "home" } })),
        )
        .expect(1)
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/user"))
        .and(wiremock::matchers::header(
            "authorization",
            "pk_work_aaaaaaaa",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "user": { "id": 1, "username": "work" } })),
        )
        .expect(1)
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/team"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "teams": [] })))
        .mount(&mock)
        .await;
    let env = Env::new(None);
    two_profiles(&env, &mock.uri());
    let (client, server) = connect().await;

    let home = call(&client, "clickup_whoami", json!({ "profile": "personal" }))
        .await
        .unwrap();
    assert_eq!(home["result"]["username"], json!("home"));
    let work = call(&client, "clickup_whoami", json!({})).await.unwrap();
    assert_eq!(work["result"]["username"], json!("work"));

    let err = call(&client, "clickup_whoami", json!({ "profile": "nope" }))
        .await
        .unwrap_err();
    assert!(
        err.contains("config_error") && err.contains("unknown profile"),
        "{err}"
    );

    let list = call(&client, "clickup_profile_list", json!({}))
        .await
        .unwrap();
    assert_eq!(list["result"]["active"], json!("work"));
    assert_eq!(list["result"]["profiles"].as_array().unwrap().len(), 2);
    assert!(!list.to_string().contains("pk_home_bbbbbbbb"));

    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
#[serial]
async fn workspace_param_resolves_name() {
    let mock = MockServer::start().await;
    Mock::given(path("/v2/team"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "teams": [
            { "id": "111", "name": "Acme" }, { "id": "333", "name": "Side Project" }
        ]})))
        .mount(&mock)
        .await;
    Mock::given(path("/v2/team/333/space"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "spaces": [] })))
        .expect(1)
        .mount(&mock)
        .await;
    let env = Env::new(None);
    two_profiles(&env, &mock.uri());
    let (client, server) = connect().await;
    let out = call(
        &client,
        "clickup_hierarchy",
        json!({ "workspace": "side project", "refresh": true }),
    )
    .await
    .unwrap();
    assert_eq!(out["result"]["spaces"], json!([]));
    client.cancel().await.unwrap();
    server.await.unwrap();
}
