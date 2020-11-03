use std::sync::Arc;

use futures::{future::BoxFuture, join, FutureExt};
use serde::{Deserialize, Serialize};

use calculator_client::{ComputeRequest, ComputeValue};
use rocket::http::Status;
use rocket::{Route, State};
use rocket_contrib::json::{Json, JsonValue};
use rocket_util::Authenticated;
use telemetry::Measure;

use crate::client::CalculatorClient;
use crate::error::ApiError;
use crate::expression::{parse, Expr};

lazy_static! {
    static ref COMPUTE_MEASURE: Measure = Measure::new("controller", "compute");
}

fn eval(
    authorization: String,
    client: Arc<CalculatorClient>,
    e: &Expr,
) -> BoxFuture<Result<ComputeValue, ApiError>> {
    // As this method is self-recursive it returns a boxed future
    match e {
        Expr::Constant(v) => futures::future::ready(Ok(*v)).boxed(),
        Expr::Application(op, l, r) => Box::pin(async move {
            let (left, right) = join!(
                eval(authorization.clone(), client.clone(), l),
                eval(authorization.clone(), client.clone(), r)
            );

            let request = ComputeRequest {
                operation: op.clone(),
                left: left?,
                right: right?,
            };

            tokio::spawn(async move { client.compute(&request, authorization).await }).await?
        }),
    }
}

#[get("/status")]
fn status() -> JsonValue {
    json!({ "status": "ok" })
}

#[get("/metrics")]
fn metrics() -> Result<String, Status> {
    telemetry::encode().map_err(|_| Status::InternalServerError)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct Expression {
    expr: String,
}

#[post("/api/v1/compute", format = "json", data = "<request>")]
async fn compute(
    authenticated: Authenticated,
    request: Json<Expression>,
    client: State<'_, Arc<CalculatorClient>>,
) -> Result<Json<ComputeValue>, ApiError> {
    COMPUTE_MEASURE
        .stats(async move {
            let expr = parse(&request.expr)?;
            let val = eval(authenticated.header, client.inner().clone(), &expr).await?;

            Ok(Json(val))
        })
        .await
}

pub fn routes() -> Vec<Route> {
    routes![status, metrics, compute]
}
