use rocket::http::Status;
use rocket::response::content;
use rocket::{Route, State};
use rocket_contrib::json::JsonValue;

use jwt::Issuer;

pub use crate::api::config::ApiConfig;
use std::sync::Arc;

mod client;
mod config;
mod error;
mod token;
mod user;

#[get("/.well-known/jwks.json")]
fn jwks(issuer: State<Arc<Issuer>>) -> content::Json<String> {
    content::Json(issuer.jwks().clone())
}

#[get("/status")]
fn status() -> JsonValue {
    json!({ "status": "ok" })
}

#[get("/metrics")]
fn metrics() -> Result<String, Status> {
    telemetry::encode().map_err(|_| Status::InternalServerError)
}

pub fn routes() -> Vec<Route> {
    let mut routes = routes![status, metrics, jwks];
    routes.append(&mut token::routes());
    routes.append(&mut client::routes());
    routes.append(&mut user::routes());
    routes
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::sync::Arc;

    use ring::rand::SystemRandom;
    use rocket::local::blocking::Client;
    use serde::Deserialize;

    use jwt::Jwks;

    use super::*;

    #[derive(Deserialize)]
    struct StatusResponse {
        status: String,
    }

    #[test]
    fn test_status() -> Result<(), Box<dyn Error>> {
        let rocket = rocket::ignite().mount("/", routes![status]);
        let client = Client::untracked(rocket).expect("valid rocket instance");
        let response = client.get("/status").dispatch();

        assert_eq!(response.status(), Status::Ok);
        let decoded: StatusResponse = serde_json::from_reader(response)?;
        assert_eq!(decoded.status, "ok");
        Ok(())
    }

    #[test]
    fn test_jwks() -> Result<(), Box<dyn Error>> {
        let rand = Arc::new(SystemRandom::new());
        let issuer = Arc::new(Issuer::test(rand)?);

        let rocket = rocket::ignite().manage(issuer).mount("/", routes![jwks]);
        let client = Client::untracked(rocket).expect("valid rocket instance");
        let response = client.get("/.well-known/jwks.json").dispatch();

        assert_eq!(response.status(), Status::Ok);
        let decoded: Jwks = serde_json::from_reader(response)?;
        assert_eq!(decoded.keys.len(), 1);
        assert_eq!(decoded.keys[0].kid, "1");

        Ok(())
    }
}
