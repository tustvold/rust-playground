use std::collections::HashSet;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

pub use dynamo::RenewalTokenDaoDynamo;
pub use memory::RenewalTokenDaoMemory;

use crate::dao::error::DaoError;
use crate::model::{RenewalToken, Scope};

mod dynamo;
mod memory;

#[async_trait]
pub trait RenewalTokenDao: Sync + Send {
    async fn generate(
        &self,
        subject: &str,
        client_id: &str,
        device_name: &str,
        scopes: HashSet<Scope>,
        expiry: DateTime<Utc>,
    ) -> Result<String, DaoError>;

    async fn consume(&self, client_id: &str, token: &str) -> Result<RenewalToken, DaoError>;
}

#[cfg(test)]
mod test {
    use std::error::Error;
    use std::sync::Arc;

    use chrono::Duration;
    use ring::rand::SystemRandom;

    use credential::CredentialService;

    use crate::service::token::TokenService;

    use super::*;

    fn clients() -> Result<Vec<Box<dyn RenewalTokenDao>>, Box<dyn Error>> {
        let figment = rocket::Config::figment();
        let config: crate::config::Config = figment.extract().unwrap();
        let client = Arc::new(config.dao.dynamo_client());
        let rand = Arc::new(SystemRandom::new());
        let credential = Arc::new(CredentialService::test()?);
        let token = Arc::new(TokenService::new(rand));

        Ok(vec![
            Box::new(RenewalTokenDaoDynamo::new(
                &config.dao,
                client,
                credential,
                token.clone(),
            )),
            Box::new(RenewalTokenDaoMemory::new(token)),
        ])
    }

    async fn get_token(
        client: &dyn RenewalTokenDao,
        expiry: i64,
    ) -> Result<String, Box<dyn Error>> {
        let scopes: HashSet<_> = [Scope::OfflineAccess].iter().cloned().collect();
        let token = client
            .generate(
                "subject",
                "client_id",
                "device_name",
                scopes.clone(),
                Utc::now() + Duration::seconds(expiry),
            )
            .await?;
        Ok(token)
    }

    #[tokio::test]
    async fn test_basic() -> Result<(), Box<dyn Error>> {
        let clients = clients()?;

        for client in clients.iter() {
            let token = get_token(client.as_ref(), 1000).await?;
            client.consume("client_id", &token).await?;
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_expiry() -> Result<(), Box<dyn Error>> {
        let clients = clients()?;

        for client in clients.iter() {
            let token = get_token(client.as_ref(), -1000).await?;

            match client.consume("client_id", &token).await {
                Err(DaoError::ExpiredCredential) => (),
                _ => panic!(),
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_duplicate_consume() -> Result<(), Box<dyn Error>> {
        let clients = clients()?;

        for client in clients.iter() {
            let token = get_token(client.as_ref(), 1000).await?;
            client.consume("client_id", &token).await?;

            match client.consume("client_id", &token).await {
                Err(DaoError::InvalidCredential) => (),
                _ => panic!(),
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_incorrect_client() -> Result<(), Box<dyn Error>> {
        let clients = clients()?;

        for client in clients.iter() {
            let token = get_token(client.as_ref(), 1000).await?;

            match client.consume("client_id2", &token).await {
                Err(DaoError::InvalidCredential) => (),
                _ => panic!(),
            }
        }

        Ok(())
    }
}
