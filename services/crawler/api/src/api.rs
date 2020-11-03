use std::rc::Rc;

use actix_http::ResponseBuilder;
use actix_web::http::{header, StatusCode};
use actix_web::{error, web, HttpResponse, Responder};
use derive_more::Display;
use serde::Deserialize;

use log::error;
use shared::dao::LinkDao;
use shared::metrics::MetricsService;
use shared::mq::MessageQueue;

#[derive(Clone)]
pub(crate) struct ApiState {
    dao: Rc<dyn LinkDao>,
    publisher: Rc<dyn MessageQueue>,
}

impl ApiState {
    pub fn new(dao: Box<dyn LinkDao>, publisher: Box<dyn MessageQueue>) -> ApiState {
        ApiState {
            dao: dao.into(),
            publisher: publisher.into(),
        }
    }
}

#[derive(Debug, Display)]
enum ApiError {
    #[display(fmt = "An internal error occurred. Please try again later.")]
    InternalError,
}

impl error::ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match *self {
            ApiError::InternalError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
    fn error_response(&self) -> HttpResponse {
        ResponseBuilder::new(self.status_code())
            .set_header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(self.to_string())
    }
}

#[derive(Deserialize)]
struct IndexRequest {
    url: String,
}

async fn index_post(
    metrics: web::Data<MetricsService>,
    state: web::Data<ApiState>,
    req: web::Json<IndexRequest>,
) -> impl Responder {
    metrics
        .stats("index_post".to_string(), move || async move {
            state
                .publisher
                .queue_index(req.url.to_string())
                .await
                .map(|_| HttpResponse::NoContent())
                .map_err(|e| {
                    error!("index_post: {}", e);
                    ApiError::InternalError
                })
        })
        .await
}

pub fn api_factory(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/index").route(web::post().to(index_post)));
}
