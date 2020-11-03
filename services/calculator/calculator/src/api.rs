use rocket::http::Status;
use rocket::Route;
use rocket_contrib::json::{Json, JsonValue};

use calculator_client::{ComputeOperation, ComputeRequest, ComputeValue};
use rocket_util::Authenticated;
use telemetry::Measure;

lazy_static! {
    static ref COMPUTE_MEASURE: Measure = Measure::new("controller", "compute");
}

#[get("/status")]
fn status() -> JsonValue {
    json!({ "status": "ok" })
}

#[get("/metrics")]
fn metrics() -> Result<String, Status> {
    telemetry::encode().map_err(|_| Status::InternalServerError)
}

#[post("/api/v1/compute", format = "json", data = "<request>")]
pub async fn compute(
    _authenticated: Authenticated,
    request: Json<ComputeRequest>,
) -> Result<Json<ComputeValue>, ()> {
    COMPUTE_MEASURE
        .stats(async move {
            let val = match request.operation {
                ComputeOperation::Add => request.left + request.right,
                ComputeOperation::Sub => request.left - request.right,
                ComputeOperation::Mul => request.left * request.right,
                ComputeOperation::Div => request.left / request.right,
            };

            Ok(Json(val))
        })
        .await
}

pub fn routes() -> Vec<Route> {
    routes![status, metrics, compute]
}
