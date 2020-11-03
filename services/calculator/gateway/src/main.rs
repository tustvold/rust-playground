#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate rocket;
#[macro_use]
extern crate rocket_contrib;

use reqwest::ClientBuilder;
use tokio::time::Duration;

use crate::client::CalculatorClient;
use jwt::Validator;
use std::sync::Arc;

mod api;
mod client;
mod config;
mod error;
mod expression;

#[rocket::main]
async fn main() {
    env_logger::init();
    let figment = rocket_util::figment();
    let config: config::Config = figment.extract().unwrap();

    let validator = Validator::new(&config.validator).expect("Failed to load JWT validator");

    let http_client = ClientBuilder::new()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("Failed to build HTTP Client");

    let client = CalculatorClient::new(http_client, config.upstream.calculator.clone());

    let result = rocket::custom(figment)
        .manage(validator)
        .manage(Arc::new(client))
        .mount("/", api::routes())
        .launch()
        .await;

    assert!(result.is_ok());
}
