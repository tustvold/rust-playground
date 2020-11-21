use rocket::http::Status;
use rocket::{Route, State};
use rocket_contrib::json::{Json, JsonValue};
use serde::{Deserialize, Serialize};

use kinesis::producer::{Error, Producer, RawRecord};
use rocket_util::Authenticated;
use telemetry::Measure;
use tracing::error;

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

#[derive(Deserialize)]
struct PutRecords {
    records: Vec<RawRecord>,
}

#[derive(Serialize)]
struct PutRecordsResponseItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    sequence_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    shard_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct PutRecordsResponse {
    results: Vec<PutRecordsResponseItem>,
}

#[post("/api/v1/records", format = "json", data = "<request>")]
async fn submit(
    _authenticated: Authenticated,
    request: Json<PutRecords>,
    producer: State<'_, Producer>,
) -> Result<Json<PutRecordsResponse>, ()> {
    let results = producer
        .inner()
        .clone()
        .submit(request.0.records.into_iter())
        .await;

    let results = results
        .into_iter()
        .map(|x| match x {
            Ok(ack) => PutRecordsResponseItem {
                sequence_number: Some(ack.sequence_number),
                shard_id: Some(ack.shard_id.to_string()),
                error: None,
            },
            Err(e) => {
                error!("producer error: {:?}", e);
                let msg = match e {
                    Error::RecordTooLarge => "Record too large",
                    Error::WorkerDead => "Internal Server Error",
                    Error::AckDropped => "Internal Server Error",
                }
                .to_string();

                PutRecordsResponseItem {
                    sequence_number: None,
                    shard_id: None,
                    error: Some(msg),
                }
            }
        })
        .collect();

    Ok(Json(PutRecordsResponse { results }))
}

pub fn routes() -> Vec<Route> {
    routes![status, metrics, submit]
}
