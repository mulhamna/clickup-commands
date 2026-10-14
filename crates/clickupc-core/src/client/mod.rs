//! HTTP plumbing: auth header, retry on 429, error-body parsing. Resource
//! methods live in sibling modules as `impl ClickUpClient` blocks.

mod comment;
mod doc;
mod field;
mod hierarchy;
mod relation;
mod task;
mod time;

pub use comment::COMMENT_PAGE_SIZE;
pub use doc::DocParent;
pub use field::{convert_field_value, relation_value, FieldValue};
pub use hierarchy::{select_workspace, workspaces_for};
pub use relation::Dependency;
pub use task::{CreateTask, MoveTask, UpdateTask, MAX_PAGES, PAGE_SIZE};
pub use time::{NewTimeEntry, TimeQuery};

use std::sync::Arc;

use reqwest::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION},
    Method, RequestBuilder, StatusCode,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{
    config::ClickUpConfig,
    error::{redact, ClickUpError, Result},
    rate_limit::{retry_delay, RateLimiter},
};

/// Retries after the first 429 before giving up with `RateLimit`.
pub const MAX_RETRIES: usize = 3;

/// Query builder that keeps insertion order and supports repeated `key[]=`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Query(Vec<(String, String)>);

impl Query {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(mut self, key: &str, value: impl ToString) -> Self {
        self.0.push((key.to_string(), value.to_string()));
        self
    }

    pub fn push_opt(self, key: &str, value: Option<impl ToString>) -> Self {
        match value {
            Some(v) => self.push(key, v),
            None => self,
        }
    }

    pub fn push_if(self, key: &str, cond: bool) -> Self {
        if cond {
            self.push(key, "true")
        } else {
            self
        }
    }

    /// `key` should include the `[]` suffix, e.g. `statuses[]`.
    pub fn push_all<I, T>(mut self, key: &str, values: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: ToString,
    {
        for v in values {
            self.0.push((key.to_string(), v.to_string()));
        }
        self
    }

    pub fn extend(mut self, other: Query) -> Self {
        self.0.extend(other.0);
        self
    }

    pub fn pairs(&self) -> &[(String, String)] {
        &self.0
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Body {
    Json(Value),
    File {
        field: &'static str,
        name: String,
        bytes: Vec<u8>,
        mime: String,
    },
}

#[derive(Clone)]
pub struct ClickUpClient {
    http: reqwest::Client,
    base: String,
    workspace_id: Option<String>,
    timezone: Option<String>,
    limiter: Arc<RateLimiter>,
}

impl std::fmt::Debug for ClickUpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClickUpClient")
            .field("base", &self.base)
            .field("workspace_id", &self.workspace_id)
            .finish_non_exhaustive()
    }
}

impl ClickUpClient {
    pub fn new(cfg: &ClickUpConfig) -> Result<Self> {
        let token = cfg.token()?;
        let mut auth = HeaderValue::from_str(token)
            .map_err(|_| ClickUpError::Auth("token contains invalid characters".into()))?;
        auth.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, auth);
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(cfg.timeout_secs))
            .user_agent(concat!("clickupc/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            http,
            base: cfg.base_url.trim_end_matches('/').to_string(),
            workspace_id: cfg.workspace_id.clone(),
            timezone: cfg.timezone.clone(),
            limiter: Arc::new(RateLimiter::default()),
        })
    }

    /// Share one limiter across clients (e.g. per-profile in the MCP server).
    pub fn with_limiter(mut self, limiter: Arc<RateLimiter>) -> Self {
        self.limiter = limiter;
        self
    }

    pub fn with_workspace(mut self, workspace_id: impl Into<String>) -> Self {
        self.workspace_id = Some(workspace_id.into());
        self
    }

    pub fn limiter(&self) -> &Arc<RateLimiter> {
        &self.limiter
    }

    pub fn timezone(&self) -> Option<&str> {
        self.timezone.as_deref()
    }

    pub fn workspace_id(&self) -> Result<&str> {
        self.workspace_id.as_deref().ok_or_else(|| {
            ClickUpError::Config(
                "no workspace selected; run `clickupc workspace use <name>` or set CLICKUP_WORKSPACE_ID"
                    .into(),
            )
        })
    }

    pub(crate) async fn get<T: DeserializeOwned>(&self, path: &str, q: &Query) -> Result<T> {
        self.call(Method::GET, path, q, None).await
    }

    pub(crate) async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        q: &Query,
        body: Value,
    ) -> Result<T> {
        self.call(Method::POST, path, q, Some(Body::Json(body)))
            .await
    }

    pub(crate) async fn put<T: DeserializeOwned>(
        &self,
        path: &str,
        q: &Query,
        body: Value,
    ) -> Result<T> {
        self.call(Method::PUT, path, q, Some(Body::Json(body)))
            .await
    }

    pub(crate) async fn delete(&self, path: &str, q: &Query) -> Result<Value> {
        self.call(Method::DELETE, path, q, None).await
    }

    pub(crate) async fn call<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        q: &Query,
        body: Option<Body>,
    ) -> Result<T> {
        let value = self.send(method, path, q, body.as_ref()).await?;
        Ok(serde_json::from_value(value)?)
    }

    fn build(&self, method: &Method, path: &str, q: &Query, body: Option<&Body>) -> RequestBuilder {
        let mut req = self
            .http
            .request(method.clone(), format!("{}{}", self.base, path));
        if !q.pairs().is_empty() {
            req = req.query(q.pairs());
        }
        match body {
            Some(Body::Json(v)) => req.json(v),
            Some(Body::File {
                field,
                name,
                bytes,
                mime,
            }) => {
                let part = reqwest::multipart::Part::bytes(bytes.clone()).file_name(name.clone());
                let part = part.mime_str(mime).unwrap_or_else(|_| {
                    reqwest::multipart::Part::bytes(bytes.clone()).file_name(name.clone())
                });
                req.multipart(reqwest::multipart::Form::new().part(*field, part))
            }
            None => req,
        }
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        q: &Query,
        body: Option<&Body>,
    ) -> Result<Value> {
        let mut attempt = 0;
        loop {
            self.limiter.wait().await;
            let resp = self.build(&method, path, q, body).send().await?;
            let status = resp.status();
            let headers = resp.headers().clone();
            self.limiter.observe(&headers);
            tracing::debug!(
                %method,
                path,
                status = status.as_u16(),
                remaining = ?self.limiter.remaining(),
                "clickup request"
            );

            if status == StatusCode::TOO_MANY_REQUESTS {
                let (delay, reset_at) = retry_delay(&headers);
                if attempt >= MAX_RETRIES {
                    return Err(ClickUpError::RateLimit { reset_at });
                }
                attempt += 1;
                tracing::debug!(?delay, attempt, "429 from ClickUp; retrying");
                tokio::time::sleep(delay).await;
                continue;
            }

            let text = resp.text().await?;
            if status.is_success() {
                if text.trim().is_empty() {
                    return Ok(Value::Null);
                }
                return Ok(serde_json::from_str(&text)?);
            }
            return Err(parse_error(status.as_u16(), &text));
        }
    }

    /// Escape hatch for `clickupc api` / `clickup_api_request`.
    /// `path` may carry a query string; `{workspace}` is substituted.
    pub async fn raw(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value> {
        let method = Method::from_bytes(method.trim().to_ascii_uppercase().as_bytes())
            .map_err(|_| ClickUpError::Invalid(format!("invalid HTTP method '{method}'")))?;
        let mut path = path.trim().to_string();
        if path.contains("{workspace}") {
            path = path.replace("{workspace}", self.workspace_id()?);
        }
        if let Some(rest) = path.strip_prefix("/api/") {
            path = format!("/{rest}");
        }
        if !path.starts_with('/') {
            return Err(ClickUpError::Invalid(
                "path must start with '/' (e.g. /v2/team)".into(),
            ));
        }
        let (path, query) = match path.split_once('?') {
            Some((p, qs)) => {
                let q = url::form_urlencoded::parse(qs.as_bytes())
                    .fold(Query::new(), |q, (k, v)| q.push(&k, v));
                (p.to_string(), q)
            }
            None => (path, Query::new()),
        };
        self.send(method, &path, &query, body.map(Body::Json).as_ref())
            .await
    }
}

/// v2: `{ "err": "...", "ECODE": "..." }`; v3: `{ "message", "trace_id", ... }`.
pub(crate) fn parse_error(status: u16, text: &str) -> ClickUpError {
    let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let str_of = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let code = str_of("ECODE").or_else(|| str_of("code"));
    let message = str_of("err")
        .or_else(|| str_of("message"))
        .or_else(|| str_of("error"))
        .unwrap_or_else(|| {
            let t = text.trim();
            if t.is_empty() {
                StatusCode::from_u16(status)
                    .ok()
                    .and_then(|s| s.canonical_reason())
                    .unwrap_or("request failed")
                    .to_string()
            } else {
                t.chars().take(500).collect()
            }
        });
    let message = redact(&message);
    let with_code = match &code {
        Some(c) => format!("{message} [{c}]"),
        None => message.clone(),
    };
    match status {
        401 => ClickUpError::Auth(with_code),
        404 => ClickUpError::NotFound(with_code),
        _ => ClickUpError::Api {
            status,
            code,
            message: if status == 403 {
                format!("{message} (this may require a higher ClickUp plan or more permissions)")
            } else {
                message
            },
            trace_id: str_of("trace_id"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v2_and_v3_errors() {
        match parse_error(400, r#"{"err":"Status not found","ECODE":"CRTSK_001"}"#) {
            ClickUpError::Api { code, message, .. } => {
                assert_eq!(code.as_deref(), Some("CRTSK_001"));
                assert_eq!(message, "Status not found");
            }
            other => panic!("unexpected {other:?}"),
        }
        match parse_error(
            422,
            r#"{"status":422,"message":"Bad page","trace_id":"t-1","timestamp":1}"#,
        ) {
            ClickUpError::Api {
                trace_id, message, ..
            } => {
                assert_eq!(trace_id.as_deref(), Some("t-1"));
                assert_eq!(message, "Bad page");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(
            parse_error(401, r#"{"err":"Token invalid","ECODE":"OAUTH_025"}"#),
            ClickUpError::Auth(m) if m.contains("OAUTH_025")
        ));
        assert!(matches!(parse_error(404, ""), ClickUpError::NotFound(_)));
        assert!(matches!(
            parse_error(500, "upstream pk_secret_123 failed"),
            ClickUpError::Api { message, .. } if !message.contains("secret")
        ));
    }

    #[test]
    fn query_builder_keeps_repeated_keys() {
        let q = Query::new()
            .push("page", 0)
            .push_all("statuses[]", ["to do", "in progress"])
            .push_opt("x", None::<String>)
            .push_if("include_closed", true);
        assert_eq!(
            q.pairs(),
            &[
                ("page".into(), "0".into()),
                ("statuses[]".into(), "to do".into()),
                ("statuses[]".into(), "in progress".into()),
                ("include_closed".into(), "true".into()),
            ]
        );
    }
}
