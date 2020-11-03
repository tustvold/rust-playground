#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate rocket;
#[macro_use]
extern crate rocket_contrib;

use std::error::Error;
use std::sync::Arc;

use ring::rand::SystemRandom;

use credential::CredentialService;
use jwt::Issuer;

use crate::dao::{
    ClientDao, ClientDaoDynamo, RenewalTokenDao, RenewalTokenDaoDynamo, UserDao, UserDaoDynamo,
};
use crate::service::AuthService;
use service::token::TokenService;

mod api;
mod config;
mod dao;
mod model;
mod policy;
mod service;

#[rocket::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let figment = rocket_util::figment();

    let config: config::Config = figment.extract().unwrap();
    let client = Arc::new(config.dao.dynamo_client());

    let rand = Arc::new(SystemRandom::new());
    let credential = Arc::new(CredentialService::new(&config.credential)?);
    let token = Arc::new(TokenService::new(rand.clone()));

    let issuer = Arc::new(Issuer::new(&config.issuer, rand.clone())?);
    let validator = issuer.new_validator().expect("Failed to get issuer");
    let user_dao = Arc::new(UserDaoDynamo::new(
        &config.dao,
        client.clone(),
        credential.clone(),
    ));

    let renewal_dao = Arc::new(RenewalTokenDaoDynamo::new(
        &config.dao,
        client.clone(),
        credential.clone(),
        token.clone(),
    ));

    let client_dao = Arc::new(ClientDaoDynamo::new(
        &config.dao,
        client.clone(),
        credential.clone(),
        token.clone(),
    ));

    let auth_service = Arc::new(AuthService::new(
        user_dao.clone(),
        client_dao.clone(),
        renewal_dao.clone(),
        issuer.clone(),
    ));

    if config.dao.seed {
        let admin_pass = token.token()?;
        user_dao.seed(&admin_pass).await?;
        client_dao.seed().await?;
    }

    rocket::custom(figment)
        .manage(issuer)
        .manage(validator)
        .manage(auth_service)
        .manage(config.api)
        .manage(client_dao as Arc<dyn ClientDao>)
        .manage(renewal_dao as Arc<dyn RenewalTokenDao>)
        .manage(user_dao as Arc<dyn UserDao>)
        .mount("/", api::routes())
        .launch()
        .await
        .expect("Rocket exited with error");

    Ok(())
}
