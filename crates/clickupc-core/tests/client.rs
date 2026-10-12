//! HTTP-level tests against a wiremock server. Never touches the real API.

use std::sync::atomic::{AtomicUsize, Ordering};

use clickupc_core::{
    client::{UpdateTask, COMMENT_PAGE_SIZE},
    config::ClickUpConfig,
    filter::{FieldFilter, Range, TaskFilter},
    rate_limit::now_secs,
    ClickUpClient, ClickUpError, Resolver, TaskRef,
};
use serde_json::{json, Value};
use wiremock::{
    matchers::{body_json, header, method, path, path_regex, query_param},
    Mock, MockServer, Request, Respond, ResponseTemplate,
};

const TOKEN: &str = "pk_test_TOKEN";
const WS: &str = "9001";

fn cfg(server: &MockServer, cache: &tempfile::TempDir) -> ClickUpConfig {
    let mut c = ClickUpConfig::with_token(TOKEN, server.uri());
    c.workspace_id = Some(WS.into());
    c.cache_dir = cache.path().to_path_buf();
    c.aliases.insert("sprint".into(), "301".into());
    c
}

fn client(server: &MockServer) -> ClickUpClient {
    let c = ClickUpConfig::with_token(TOKEN, server.uri());
    ClickUpClient::new(&c).unwrap().with_workspace(WS)
}

fn task(id: u32) -> Value {
    json!({ "id": format!("t{id}"), "name": format!("Task {id}"), "status": { "status": "open" } })
}

fn tasks(range: std::ops::Range<u32>) -> Vec<Value> {
    range.map(task).collect()
}

fn rate_limited() -> ResponseTemplate {
    ResponseTemplate::new(429)
        .insert_header("x-ratelimit-remaining", "0")
        .insert_header("x-ratelimit-reset", now_secs().to_string())
        .set_body_json(json!({ "err": "Rate limit reached", "ECODE": "APP_002" }))
}

/// Responds 429 for the first `n` calls, then 200 with `body`.
struct FailFirst {
    n: usize,
    calls: AtomicUsize,
    body: Value,
}

impl Respond for FailFirst {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        if self.calls.fetch_add(1, Ordering::SeqCst) < self.n {
            rate_limited()
        } else {
            ResponseTemplate::new(200).set_body_json(self.body.clone())
        }
    }
}

#[tokio::test]
async fn auth_header_is_raw_token() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/user"))
        .and(header("authorization", TOKEN))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "user": { "id": 1, "username": "me" } })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let user = client(&server).user().await.unwrap();
    assert_eq!(user.id, "1");
    let req = &server.received_requests().await.unwrap()[0];
    let auth = req.headers.get("authorization").unwrap().to_str().unwrap();
    assert!(!auth.starts_with("Bearer"));
}

#[tokio::test]
async fn retries_after_429_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/user"))
        .respond_with(FailFirst {
            n: 1,
            calls: AtomicUsize::new(0),
            body: json!({ "user": { "id": 7 } }),
        })
        .expect(2)
        .mount(&server)
        .await;
    assert_eq!(client(&server).user().await.unwrap().id, "7");
}

#[tokio::test]
async fn gives_up_after_four_429s() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/user"))
        .respond_with(rate_limited())
        .expect(4)
        .mount(&server)
        .await;
    let err = client(&server).user().await.unwrap_err();
    assert!(matches!(err, ClickUpError::RateLimit { .. }), "{err:?}");
}

#[tokio::test]
async fn v2_and_v3_error_bodies() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/list/1"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(json!({ "err": "List not found", "ECODE": "ITEM_013" })),
        )
        .mount(&server)
        .await;
    Mock::given(path_regex("^/v3/workspaces/9001/docs/d1$"))
        .respond_with(ResponseTemplate::new(422).set_body_json(
            json!({ "status": 422, "message": "Invalid doc", "trace_id": "abc", "timestamp": 1 }),
        ))
        .mount(&server)
        .await;
    let c = client(&server);
    match c.list("1").await.unwrap_err() {
        ClickUpError::Api { status, code, .. } => {
            assert_eq!(status, 400);
            assert_eq!(code.as_deref(), Some("ITEM_013"));
        }
        e => panic!("{e:?}"),
    }
    match c.get_doc("d1").await.unwrap_err() {
        ClickUpError::Api {
            trace_id, message, ..
        } => {
            assert_eq!(trace_id.as_deref(), Some("abc"));
            assert_eq!(message, "Invalid doc");
        }
        e => panic!("{e:?}"),
    }
}

async fn mount_hierarchy(server: &MockServer, times: u64) {
    Mock::given(path(format!("/v2/team/{WS}/space")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "spaces": [
            { "id": "100", "name": "Engineering" },
            { "id": "200", "name": "Marketing" }
        ]})))
        .expect(times)
        .mount(server)
        .await;
    Mock::given(path("/v2/space/100/folder"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "folders": [
                { "id": "110", "name": "Sprints", "lists": [
                    { "id": "301", "name": "Sprint 12" },
                    { "id": "302", "name": "Backlog" }
                ]}
            ]})),
        )
        .expect(times)
        .mount(server)
        .await;
    Mock::given(path("/v2/space/100/list"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "lists": [
            { "id": "303", "name": "Inbox" }
        ]})))
        .expect(times)
        .mount(server)
        .await;
    Mock::given(path("/v2/space/200/folder"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "folders": [] })))
        .expect(times)
        .mount(server)
        .await;
    Mock::given(path("/v2/space/200/list"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "lists": [
            { "id": "401", "name": "Backlog" }
        ]})))
        .expect(times)
        .mount(server)
        .await;
}

#[tokio::test]
async fn hierarchy_walk_then_cache_hit() {
    let server = MockServer::start().await;
    let cache = tempfile::tempdir().unwrap();
    mount_hierarchy(&server, 1).await;
    let cfg = cfg(&server, &cache);
    let c = ClickUpClient::new(&cfg).unwrap();

    let tree = Resolver::new(&c, &cfg)
        .unwrap()
        .hierarchy(false)
        .await
        .unwrap();
    assert_eq!(tree.lists().len(), 4);
    assert_eq!(server.received_requests().await.unwrap().len(), 5);

    // A new resolver reads the disk cache: zero HTTP.
    let again = Resolver::new(&c, &cfg)
        .unwrap()
        .hierarchy(false)
        .await
        .unwrap();
    assert_eq!(again, tree);
    assert_eq!(server.received_requests().await.unwrap().len(), 5);
}

#[tokio::test]
async fn resolver_list_inputs() {
    let server = MockServer::start().await;
    let cache = tempfile::tempdir().unwrap();
    mount_hierarchy(&server, 1).await;
    let cfg = cfg(&server, &cache);
    let c = ClickUpClient::new(&cfg).unwrap();
    let r = Resolver::new(&c, &cfg).unwrap();

    assert_eq!(r.list("inbox").await.unwrap().id, "303"); // unique name (walks tree)
    assert_eq!(r.list("303").await.unwrap().name, "Inbox"); // exact id, name from cache
    assert_eq!(r.list("999").await.unwrap().name, "999"); // unknown id is trusted
    assert_eq!(r.list("sprint").await.unwrap().id, "301"); // alias
    assert_eq!(
        r.list("Engineering/Sprints/Backlog").await.unwrap().id,
        "302"
    ); // path
    assert_eq!(r.list("Marketing/Backlog").await.unwrap().id, "401"); // folderless path
    assert_eq!(server.received_requests().await.unwrap().len(), 5);

    match r.list("Backlog").await.unwrap_err() {
        e @ ClickUpError::Resolve { .. } => {
            let msg = e.to_string();
            assert!(msg.contains("Engineering/Sprints/Backlog"), "{msg}");
            assert!(msg.contains("Marketing/Backlog"), "{msg}");
        }
        e => panic!("{e:?}"),
    }
    // Typo: fresh tree, so no refetch; suggestion returned.
    match r.list("Sprnt 12").await.unwrap_err() {
        ClickUpError::Resolve { suggestions, .. } => {
            assert!(
                suggestions.contains(&"Sprint 12".to_string()),
                "{suggestions:?}"
            )
        }
        e => panic!("{e:?}"),
    }
}

#[tokio::test]
async fn resolver_refreshes_cached_tree_once_on_miss() {
    let server = MockServer::start().await;
    let cache = tempfile::tempdir().unwrap();
    mount_hierarchy(&server, 2).await;
    let cfg = cfg(&server, &cache);
    let c = ClickUpClient::new(&cfg).unwrap();
    Resolver::new(&c, &cfg)
        .unwrap()
        .hierarchy(false)
        .await
        .unwrap();

    // New resolver: tree comes from disk, miss triggers exactly one refetch.
    let r = Resolver::new(&c, &cfg).unwrap();
    assert!(r.list("Does Not Exist").await.is_err());
}

#[tokio::test]
async fn status_resolution_and_suggestion() {
    let server = MockServer::start().await;
    let cache = tempfile::tempdir().unwrap();
    Mock::given(path("/v2/list/301"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "301", "name": "Sprint 12",
            "statuses": [
                { "status": "to do", "type": "open" },
                { "status": "in progress", "type": "custom" },
                { "status": "complete", "type": "closed" }
            ]
        })))
        .mount(&server)
        .await;
    let cfg = cfg(&server, &cache);
    let c = ClickUpClient::new(&cfg).unwrap();
    let r = Resolver::new(&c, &cfg).unwrap();
    assert_eq!(r.status("301", "In Progress").await.unwrap(), "in progress");
    match r.status("301", "in progres").await.unwrap_err() {
        ClickUpError::Resolve { suggestions, .. } => {
            assert_eq!(suggestions[0], "in progress")
        }
        e => panic!("{e:?}"),
    }
}

#[tokio::test]
async fn single_list_search_uses_list_endpoint_and_stops_on_last_page() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/list/301/task"))
        .and(query_param("page", "0"))
        .and(query_param("statuses[]", "in progress"))
        .and(query_param("include_closed", "true"))
        .and(query_param("due_date_gt", "1000"))
        .and(query_param("due_date_lt", "2000"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "tasks": tasks(0..100), "last_page": false })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/v2/list/301/task"))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "tasks": tasks(100..200), "last_page": true })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let f = TaskFilter {
        list_ids: vec!["301".into()],
        statuses: vec!["in progress".into()],
        include_closed: true,
        due: Range {
            gt: Some(1000),
            lt: Some(2000),
        },
        ..Default::default()
    };
    assert_eq!(client(&server).search_tasks(&f).await.unwrap().len(), 200);
    let raw = server.received_requests().await.unwrap()[0]
        .url
        .query()
        .unwrap()
        .to_string();
    assert!(
        raw.contains("statuses%5B%5D=in+progress") || raw.contains("statuses%5B%5D=in%20progress"),
        "{raw}"
    );
}

#[tokio::test]
async fn multi_list_search_uses_team_endpoint_and_stops_on_short_page() {
    let server = MockServer::start().await;
    let field_json = r#"[{"field_id":"f1","operator":"=","value":"o1"}]"#;
    Mock::given(path(format!("/v2/team/{WS}/task")))
        .and(query_param("page", "0"))
        .and(query_param("list_ids[]", "301"))
        .and(query_param("list_ids[]", "302"))
        .and(query_param("custom_fields", field_json))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tasks": tasks(0..100) })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path(format!("/v2/team/{WS}/task")))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tasks": tasks(100..130) })))
        .expect(1)
        .mount(&server)
        .await;
    let f = TaskFilter {
        list_ids: vec!["301".into(), "302".into()],
        custom_fields: vec![FieldFilter {
            field_id: "f1".into(),
            operator: "=".into(),
            value: json!("o1"),
        }],
        ..Default::default()
    };
    assert_eq!(client(&server).search_tasks(&f).await.unwrap().len(), 130);
}

#[tokio::test]
async fn search_respects_limit() {
    let server = MockServer::start().await;
    Mock::given(path(format!("/v2/team/{WS}/task")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tasks": tasks(0..100) })))
        .expect(1)
        .mount(&server)
        .await;
    let f = TaskFilter {
        limit: Some(10),
        ..Default::default()
    };
    assert_eq!(client(&server).search_tasks(&f).await.unwrap().len(), 10);
}

#[tokio::test]
async fn custom_id_operations_send_team_id() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/task/DEV-123"))
        .and(query_param("custom_task_ids", "true"))
        .and(query_param("team_id", WS))
        .respond_with(ResponseTemplate::new(200).set_body_json(task(1)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/v2/task/DEV-123"))
        .and(query_param("custom_task_ids", "true"))
        .and(query_param("team_id", WS))
        .and(body_json(
            json!({ "assignees": { "add": [42], "rem": [] } }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(task(1)))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    let r: TaskRef = "DEV-123".parse().unwrap();
    c.get_task(&r, false, false).await.unwrap();
    c.update_task(
        &r,
        &UpdateTask {
            assignees_add: vec![42],
            ..Default::default()
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn move_task_uses_v3_home_list() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path(format!("/v3/workspaces/{WS}/tasks/abc/home_list/302")))
        .and(body_json(json!({ "status_mappings": [
            { "source_status_id": "s1", "destination_status_id": "s2" }
        ]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    client(&server)
        .move_task(
            &"abc".parse().unwrap(),
            "302",
            &clickupc_core::client::MoveTask {
                move_custom_fields: None,
                status_mappings: vec![("s1".into(), "s2".into())],
            },
        )
        .await
        .unwrap();
}

/// 60 comments, newest first, paged with `start` / `start_id`.
struct CommentPages;

impl Respond for CommentPages {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let start_id: Option<usize> = req
            .url
            .query_pairs()
            .find(|(k, _)| k == "start_id")
            .and_then(|(_, v)| v.trim_start_matches('c').parse().ok());
        let from = start_id.map(|id| 60 - id).unwrap_or(0);
        let page: Vec<Value> = (from..(from + COMMENT_PAGE_SIZE).min(60))
            .map(|i| {
                let id = 60 - i - 1;
                json!({ "id": format!("c{id}"), "comment_text": format!("#{id}"), "date": (1000 + id).to_string() })
            })
            .collect();
        ResponseTemplate::new(200).set_body_json(json!({ "comments": page }))
    }
}

#[tokio::test]
async fn comment_pagination_across_three_pages() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/task/abc/comment"))
        .respond_with(CommentPages)
        .expect(3)
        .mount(&server)
        .await;
    let comments = client(&server)
        .task_comments(&"abc".parse().unwrap(), None)
        .await
        .unwrap();
    assert_eq!(comments.len(), 60);
    assert_eq!(comments.first().unwrap().id, "c59");
    assert_eq!(comments.last().unwrap().id, "c0");
    let reqs = server.received_requests().await.unwrap();
    let q = reqs[1].url.query().unwrap();
    assert!(
        q.contains("start=1035") && q.contains("start_id=c35"),
        "{q}"
    );
}

#[tokio::test]
async fn bulk_update_250_with_429_midway() {
    let server = MockServer::start().await;
    // Every task update succeeds, except the shared mock rate-limits once.
    Mock::given(method("PUT"))
        .and(path_regex("^/v2/task/t\\d+$"))
        .respond_with(FailFirst {
            n: 1,
            calls: AtomicUsize::new(0),
            body: task(0),
        })
        .mount(&server)
        .await;
    let c = client(&server);
    let ids: Vec<TaskRef> = (0..250).map(|i| TaskRef::Id(format!("t{i}"))).collect();
    let update = UpdateTask {
        status: Some("done".into()),
        ..Default::default()
    };
    let done = AtomicUsize::new(0);
    let results = clickupc_core::bulk::run(
        c.limiter(),
        ids,
        |t| {
            let c = &c;
            let update = &update;
            async move { c.update_task(&t, update).await }
        },
        || {
            done.fetch_add(1, Ordering::SeqCst);
        },
    )
    .await;
    assert_eq!(results.len(), 250);
    assert!(results.iter().all(Result::is_ok));
    assert_eq!(done.load(Ordering::SeqCst), 250);
    assert_eq!(server.received_requests().await.unwrap().len(), 251);
}

#[tokio::test]
async fn raw_request_substitutes_workspace() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/team/{WS}/space")))
        .and(query_param("archived", "false"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "spaces": [] })))
        .expect(1)
        .mount(&server)
        .await;
    let v = client(&server)
        .raw("get", "/v2/team/{workspace}/space?archived=false", None)
        .await
        .unwrap();
    assert_eq!(v, json!({ "spaces": [] }));
}

#[tokio::test]
async fn find_workspace_by_name_or_id() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/team"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "teams": [
            { "id": "111", "name": "Acme" },
            { "id": "222", "name": "Side Project" }
        ]})))
        .mount(&server)
        .await;
    let c = client(&server);
    assert_eq!(c.find_workspace("side project").await.unwrap().id, "222");
    assert_eq!(c.find_workspace("111").await.unwrap().name, "Acme");
    match c.find_workspace("Sde Projct").await.unwrap_err() {
        ClickUpError::Resolve { suggestions, .. } => assert_eq!(suggestions[0], "Side Project"),
        e => panic!("{e:?}"),
    }
}
