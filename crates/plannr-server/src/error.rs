use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    BadRequest(String),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    Internal(#[from] anyhow::Error),
}

impl From<plannr_core::ops::OpError> for ApiError {
    fn from(e: plannr_core::ops::OpError) -> Self { ApiError::BadRequest(e.0) }
}
impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self { ApiError::BadRequest(e.to_string()) }
}

impl ApiError {
    pub fn status(&self) -> StatusCode {
        match self {
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Db(rusqlite::Error::QueryReturnedNoRows) => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let msg = match &self {
            ApiError::Db(rusqlite::Error::QueryReturnedNoRows) => "project not found".to_string(),
            e => e.to_string(),
        };
        if status.is_server_error() { tracing::error!("{msg}"); }
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
