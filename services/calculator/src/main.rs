#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate rocket;
#[macro_use]
extern crate rocket_contrib;

use jwt::Validator;

mod api;
mod config;

#[rocket::main]
async fn main() {
    env_logger::init();

    let figment = rocket_util::figment();
    let config: config::Config = figment.extract().unwrap();

    let validator = Validator::new(&config.validator).expect("Failed to load JWT validator");

    let result = rocket::custom(figment)
        .manage(validator)
        .mount("/", api::routes())
        .launch()
        .await;

    assert!(result.is_ok());
}
