//! Tool errors with stable string codes (`data.code`) so clients can branch
//! on them. 4xx-style problems → `INVALID_PARAMS`, server-side → `INTERNAL_ERROR`.

use clickupc_core::ClickUpError;
use rmcp::{model::ErrorCode, ErrorData};
use serde_json::{json, Map, Value};

#[derive(Debug, Clone)]
pub struct AppError {
    rpc_code: ErrorCode,
    stable_code: &'static str,
    detail: String,
    data: Option<Value>,
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    fn new(
        rpc_code: ErrorCode,
        stable_code: &'static str,
        detail: impl Into<String>,
        data: Option<Value>,
    ) -> Self {
        Self {
            rpc_code,
            stable_code,
            detail: detail.into(),
            data,
        }
    }

    pub fn validation(detail: impl Into<String>) -> Self {
        Self::new(ErrorCode::INVALID_PARAMS, "validation_error", detail, None)
    }

    pub fn unsafe_operation(detail: impl Into<String>) -> Self {
        Self::new(ErrorCode::INVALID_PARAMS, "unsafe_operation", detail, None)
    }

    pub fn auth_missing(detail: impl Into<String>) -> Self {
        Self::new(ErrorCode::INVALID_REQUEST, "auth_missing", detail, None)
    }

    pub fn config_error(detail: impl Into<String>) -> Self {
        Self::new(ErrorCode::INVALID_REQUEST, "config_error", detail, None)
    }

    pub fn internal(detail: impl Into<String>) -> Self {
        Self::new(ErrorCode::INTERNAL_ERROR, "internal_error", detail, None)
    }

    pub fn code(&self) -> &'static str {
        self.stable_code
    }

    pub fn data(&self) -> Option<&Value> {
        self.data.as_ref()
    }

    pub fn to_mcp(self) -> ErrorData {
        let mut payload = match self.data {
            Some(Value::Object(map)) => map,
            Some(other) => {
                let mut m = Map::new();
                m.insert("context".into(), other);
                m
            }
            None => Map::new(),
        };
        payload.insert("code".into(), json!(self.stable_code));
        ErrorData::new(
            self.rpc_code,
            format!("{}: {}", self.stable_code, self.detail),
            Some(Value::Object(payload)),
        )
    }
}

fn rpc_for_status(status: u16) -> ErrorCode {
    if status >= 500 {
        ErrorCode::INTERNAL_ERROR
    } else {
        ErrorCode::INVALID_PARAMS
    }
}

impl From<ClickUpError> for AppError {
    fn from(err: ClickUpError) -> Self {
        let detail = err.to_string();
        match err {
            ClickUpError::Auth(_) => Self::auth_missing(detail),
            ClickUpError::Config(_) => Self::config_error(detail),
            ClickUpError::Invalid(_) => Self::validation(detail),
            ClickUpError::Resolve { suggestions, .. } => Self::new(
                ErrorCode::INVALID_PARAMS,
                "resolve_error",
                detail,
                Some(json!({ "suggestions": suggestions })),
            ),
            ClickUpError::NotFound(_) => Self::new(
                ErrorCode::INVALID_PARAMS,
                "clickup_api_error",
                detail,
                Some(json!({ "status": 404 })),
            ),
            ClickUpError::Api {
                status,
                code,
                trace_id,
                ..
            } => Self::new(
                rpc_for_status(status),
                "clickup_api_error",
                detail,
                Some(json!({ "status": status, "ecode": code, "trace_id": trace_id })),
            ),
            ClickUpError::RateLimit { reset_at } => Self::new(
                ErrorCode::INTERNAL_ERROR,
                "rate_limited",
                detail,
                Some(json!({ "reset_at": reset_at })),
            ),
            ClickUpError::Http(_) | ClickUpError::Serialization(_) | ClickUpError::Io(_) => {
                Self::internal(detail)
            }
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        Self::internal(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_core_errors_to_stable_codes() {
        let e: AppError = ClickUpError::resolve("list", "x", vec!["y".into()]).into();
        assert_eq!(e.code(), "resolve_error");
        assert_eq!(e.data().unwrap()["suggestions"], json!(["y"]));

        let e: AppError = ClickUpError::Api {
            status: 400,
            code: Some("ITEM_013".into()),
            message: "bad".into(),
            trace_id: None,
        }
        .into();
        assert_eq!(e.code(), "clickup_api_error");
        assert_eq!(e.data().unwrap()["ecode"], json!("ITEM_013"));
        assert_eq!(e.clone().to_mcp().code, ErrorCode::INVALID_PARAMS);

        let e: AppError = ClickUpError::Api {
            status: 503,
            code: None,
            message: "down".into(),
            trace_id: Some("t".into()),
        }
        .into();
        assert_eq!(e.to_mcp().code, ErrorCode::INTERNAL_ERROR);

        let e: AppError = ClickUpError::RateLimit { reset_at: 5 }.into();
        assert_eq!(e.code(), "rate_limited");
        let mcp = e.to_mcp();
        assert_eq!(mcp.data.unwrap()["reset_at"], json!(5));

        let e: AppError = ClickUpError::Auth("no token".into()).into();
        assert_eq!(e.code(), "auth_missing");
    }
}
