use std::borrow::Cow;

use log::error;
use rocket::http::Status;
use rocket::{response, Request};
use rocket_contrib::json::Json;
use serde::Serialize;

use telemetry::IsErr;

use crate::expression::ParseError;
use tokio::task::JoinError;

#[derive(Debug)]
pub enum ApiError {
    InternalError(String),
    InvalidExpression(String),
}

impl From<reqwest::Error> for ApiError {
    fn from(e: reqwest::Error) -> Self {
        ApiError::InternalError(format!("Reqwest Error: {}", e))
    }
}

impl From<ParseError> for ApiError {
    fn from(e: ParseError) -> Self {
        ApiError::InvalidExpression(e.0)
    }
}

impl From<JoinError> for ApiError {
    fn from(e: JoinError) -> Self {
        ApiError::InternalError(format!("Join Error: {}", e))
    }
}

impl IsErr for ApiError {
    fn is_err(&self) -> bool {
        matches!(self, ApiError::InternalError(_))
    }
}

#[derive(Serialize)]
struct ErrorResponse<'a> {
    message: Cow<'a, str>,
}

impl<'r> response::Responder<'r, 'static> for ApiError {
    fn respond_to(self, req: &'r Request<'_>) -> response::Result<'static> {
        let (message, status) = match self {
            ApiError::InternalError(e) => {
                error!("Internal Error: {}", e);
                (
                    Cow::Borrowed("Internal Server Error"),
                    Status::InternalServerError,
                )
            }
            ApiError::InvalidExpression(e) => (Cow::Owned(e), Status::BadRequest),
        };
        response::status::Custom(status, Json(ErrorResponse { message })).respond_to(req)
    }
}
