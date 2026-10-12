//! Runs the real `clickupc` binary against a wiremock server.

use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Output, Stdio},
    time::Duration,
};

use serde_json::{json, Value};
use wiremock::{
    matchers::{method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

struct Env {
    dir: tempfile::TempDir,
    base: Option<String>,
}

impl Env {
    fn new(base: Option<String>) -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
            base,
        }
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_clickupc"));
        c.args(args)
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("CLICKUP_CONFIG_DIR", self.dir.path().join("config"))
            .env("CLICKUP_CACHE_DIR", self.dir.path().join("cache"))
            .env("CLICKUP_TIMEZONE", "UTC")
            .stdin(Stdio::null());
        if let Some(b) = &self.base {
            c.env("CLICKUP_BASE_URL", b)
                .env("CLICKUP_TOKEN", "pk_cli_test_token")
                .env("CLICKUP_WORKSPACE_ID", "9001");
        }
        c
    }

    async fn run(&self, args: &[&str]) -> Output {
        let mut c = self.cmd(args);
        tokio::task::spawn_blocking(move || c.output().unwrap())
            .await
            .unwrap()
    }
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn version() {
    let o = Env::new(None).run(&["--version"]).await;
    assert!(o.status.success());
    assert!(stdout(&o).starts_with("clickupc "));
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_status_without_config() {
    let o = Env::new(None).run(&["auth", "status", "--json"]).await;
    assert!(o.status.success(), "{o:?}");
    let v: Value = serde_json::from_str(&stdout(&o)).unwrap();
    assert_eq!(v["token"], Value::Null);
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_token_exits_with_auth_code() {
    let o = Env::new(None).run(&["whoami"]).await;
    assert_eq!(o.status.code(), Some(3), "{o:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn task_list_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/list/301/task"))
        .and(query_param("statuses[]", "open"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tasks": [{ "id": "abc", "name": "A", "status": { "status": "open" } }],
            "last_page": true
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/list/301"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "301", "name": "L", "statuses": [{ "status": "open", "type": "open" }]
        })))
        .mount(&server)
        .await;
    let env = Env::new(Some(server.uri()));
    let o = env
        .run(&[
            "task", "list", "--list", "301", "--status", "OPEN", "--json",
        ])
        .await;
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: Value = serde_json::from_str(&stdout(&o)).unwrap();
    assert_eq!(v[0]["id"], json!("abc"));
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_refuses_without_yes_when_not_a_tty() {
    let server = MockServer::start().await;
    let env = Env::new(Some(server.uri()));
    let o = env.run(&["task", "delete", "abc"]).await;
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("--yes"));
    assert!(server.received_requests().await.unwrap().is_empty());

    Mock::given(method("DELETE"))
        .and(path("/v2/task/abc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    let o = env.run(&["task", "delete", "abc", "--yes"]).await;
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}

#[tokio::test(flavor = "multi_thread")]
async fn errors_never_print_the_token() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/user"))
        .respond_with(ResponseTemplate::new(500).set_body_string("echo pk_cli_test_token"))
        .mount(&server)
        .await;
    let o = Env::new(Some(server.uri()))
        .run(&["whoami", "--verbose"])
        .await;
    let all = format!("{}{}", stdout(&o), String::from_utf8_lossy(&o.stderr));
    assert!(!all.contains("pk_cli_test_token"), "{all}");
}

#[tokio::test(flavor = "multi_thread")]
async fn mcp_install_snippet_has_no_token() {
    let env = Env::new(Some("http://127.0.0.1:9".into()));
    let o = env
        .run(&["mcp", "install", "--client", "generic-json"])
        .await;
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = stdout(&o);
    assert!(
        out.contains("\"mcp\"") && out.contains("\"serve\""),
        "{out}"
    );
    assert!(!out.contains("pk_"), "{out}");
}

#[test]
fn mcp_serve_stdout_is_clean_until_initialize() {
    let env = Env::new(None);
    let mut child = env
        .cmd(&["mcp", "serve"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let mut stdin = child.stdin.take().unwrap();
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2025-06-18","capabilities":{{}},"clientInfo":{{"name":"t","version":"0"}}}}}}"#
    )
    .unwrap();
    let mut first = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut first)
        .unwrap();
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    let v: Value = serde_json::from_str(&first).expect("first stdout line is the initialize reply");
    assert_eq!(v["id"], json!(1));
    assert_eq!(v["result"]["serverInfo"]["name"], json!("clickupc"));
}
