//! Human-input operations against a mock server.

use std::collections::BTreeMap;

use clickupc_core::{config::ClickUpConfig, ops, ClickUpClient, ClickUpError, Resolver};
use serde_json::json;
use wiremock::{
    matchers::{method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

const WS: &str = "9001";

fn setup(server: &MockServer, cache: &tempfile::TempDir) -> (ClickUpClient, ClickUpConfig) {
    let mut cfg = ClickUpConfig::with_token("pk_x", server.uri());
    cfg.workspace_id = Some(WS.into());
    cfg.cache_dir = cache.path().to_path_buf();
    cfg.timezone = Some("UTC".into());
    (ClickUpClient::new(&cfg).unwrap(), cfg)
}

async fn mount_task_and_lists(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/v2/task/abc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "abc", "name": "A",
            "status": { "id": "st-review", "status": "review", "type": "custom" },
            "list": { "id": "301", "name": "Sprint" }
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/list/301"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "301", "name": "Sprint",
            "statuses": [
                { "id": "st-todo", "status": "to do", "type": "open" },
                { "id": "st-review", "status": "review", "type": "custom" }
            ]
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/list/302"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "302", "name": "Done",
            "statuses": [
                { "id": "st-open", "status": "open", "type": "open" },
                { "id": "st-closed", "status": "closed", "type": "closed" }
            ]
        })))
        .mount(server)
        .await;
}

#[tokio::test]
async fn set_status_typo_suggests() {
    let server = MockServer::start().await;
    let cache = tempfile::tempdir().unwrap();
    mount_task_and_lists(&server).await;
    let (c, cfg) = setup(&server, &cache);
    let r = Resolver::new(&c, &cfg).unwrap();
    match ops::set_status(&r, &"abc".parse().unwrap(), "reveiw")
        .await
        .unwrap_err()
    {
        ClickUpError::Resolve { suggestions, .. } => assert_eq!(suggestions[0], "review"),
        e => panic!("{e:?}"),
    }
}

#[tokio::test]
async fn move_without_mapping_lists_target_statuses() {
    let server = MockServer::start().await;
    let cache = tempfile::tempdir().unwrap();
    mount_task_and_lists(&server).await;
    let (c, cfg) = setup(&server, &cache);
    let r = Resolver::new(&c, &cfg).unwrap();
    match ops::move_task(&r, &"abc".parse().unwrap(), "302", &BTreeMap::new(), false)
        .await
        .unwrap_err()
    {
        ClickUpError::Resolve {
            kind, suggestions, ..
        } => {
            assert_eq!(kind, "target status");
            assert_eq!(suggestions, vec!["open", "closed"]);
        }
        e => panic!("{e:?}"),
    }
}

#[tokio::test]
async fn move_with_mapping_sends_status_ids() {
    let server = MockServer::start().await;
    let cache = tempfile::tempdir().unwrap();
    mount_task_and_lists(&server).await;
    Mock::given(method("PUT"))
        .and(path(format!("/v3/workspaces/{WS}/tasks/abc/home_list/302")))
        .and(wiremock::matchers::body_json(json!({ "status_mappings": [
            { "source_status_id": "st-review", "destination_status_id": "st-open" }
        ]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    let (c, cfg) = setup(&server, &cache);
    let r = Resolver::new(&c, &cfg).unwrap();
    let map = BTreeMap::from([("Review".to_string(), "open".to_string())]);
    ops::move_task(&r, &"abc".parse().unwrap(), "302", &map, false)
        .await
        .unwrap();
}

#[tokio::test]
async fn create_resolves_names_and_dates() {
    let server = MockServer::start().await;
    let cache = tempfile::tempdir().unwrap();
    mount_task_and_lists(&server).await;
    Mock::given(method("GET"))
        .and(path("/v2/team"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "teams": [
            { "id": WS, "name": "W", "members": [{ "user": { "id": 42, "username": "Ana", "email": "ana@example.com" } }] }
        ]})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/list/301/task"))
        .and(wiremock::matchers::body_json(json!({
            "name": "Ship it",
            "assignees": [42],
            "status": "to do",
            "priority": 2,
            "due_date": 1_792_454_400_000i64,
            "due_date_time": false,
            "time_estimate": 7_200_000
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "new", "name": "Ship it", "status": { "status": "to do" }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let (c, cfg) = setup(&server, &cache);
    let r = Resolver::new(&c, &cfg).unwrap();
    let input = ops::TaskInput {
        name: Some("Ship it".into()),
        status: Some("TO DO".into()),
        priority: Some("high".into()),
        assignees: vec!["ana".into()],
        due: Some("2026-10-20".into()),
        estimate: Some("2h".into()),
        ..Default::default()
    };
    let t = ops::create_task(&r, "301", &input).await.unwrap();
    assert_eq!(t.id, "new");
}

#[tokio::test]
async fn single_list_filter_validates_status_and_query() {
    let server = MockServer::start().await;
    let cache = tempfile::tempdir().unwrap();
    mount_task_and_lists(&server).await;
    Mock::given(method("GET"))
        .and(path("/v2/list/301/task"))
        .and(query_param("statuses[]", "review"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "tasks": [], "last_page": true })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let (c, cfg) = setup(&server, &cache);
    let r = Resolver::new(&c, &cfg).unwrap();
    let input = ops::SearchInput {
        lists: vec!["301".into()],
        statuses: vec!["REVIEW".into()],
        ..Default::default()
    };
    assert!(ops::search(&r, &input).await.unwrap().is_empty());
}
