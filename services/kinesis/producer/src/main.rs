#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate rocket;
#[macro_use]
extern crate rocket_contrib;

use jwt::Validator;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

mod api;
mod config;

#[rocket::main]
async fn main() {
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let figment = rocket_util::figment();
    let config: config::Config = figment.extract().unwrap();

    let (producer, handle) = config.kinesis.pipeline();

    let validator = Validator::new(&config.validator).expect("Failed to load JWT validator");

    let result = rocket::custom(figment)
        .manage(validator)
        .manage(producer)
        .mount("/", api::routes())
        .launch()
        .await;

    handle.shutdown().await.unwrap();

    assert!(result.is_ok());
}
