use std::collections::HashSet;

use async_trait::async_trait;

pub use dynamo::ClientDaoDynamo;
pub use memory::ClientDaoMemory;

use crate::dao::error::DaoError;
use crate::model::{Client, GrantType, Scope};

mod dynamo;
mod memory;

#[async_trait]
pub trait ClientDao: Sync + Send {
    async fn register(
        &self,
        client_name: String,
        scopes: HashSet<Scope>,
        grants: HashSet<GrantType>,
        generate_credential: bool,
        loopback: bool,
        client_id: Option<String>,
    ) -> Result<(String, Option<String>), DaoError>;

    async fn update(
        &self,
        client_id: &str,
        client_name: String,
        scopes: HashSet<Scope>,
        grants: HashSet<GrantType>,
        loopback: bool,
    ) -> Result<(), DaoError>;

    async fn lookup(&self, client_id: &str) -> Result<Option<Client>, DaoError>;

    async fn verify(
        &self,
        client_id: &str,
        token: &str,
        hashed_token: &[u8],
    ) -> Result<(), DaoError>;
}

#[cfg(test)]
mod test {
    use std::error::Error;
    use std::sync::Arc;

    use ring::rand::SystemRandom;

    use credential::CredentialService;

    use crate::service::token::TokenService;

    use super::*;

    fn clients() -> Result<Vec<Box<dyn ClientDao>>, Box<dyn Error>> {
        let figment = rocket::Config::figment();
        let config: crate::config::Config = figment.extract().unwrap();
        let client = Arc::new(config.dao.dynamo_client());
        let rand = Arc::new(SystemRandom::new());
        let credential = Arc::new(CredentialService::test()?);
        let token = Arc::new(TokenService::new(rand));

        Ok(vec![
            Box::new(ClientDaoDynamo::new(
                &config.dao,
                client,
                credential,
                token.clone(),
            )),
            Box::new(ClientDaoMemory::new(token)),
        ])
    }

    #[tokio::test]
    async fn test_client_register() -> Result<(), Box<dyn Error>> {
        let clients = clients()?;

        for client in clients.iter() {
            let grants: HashSet<_> = [GrantType::RefreshToken, GrantType::Password]
                .iter()
                .cloned()
                .collect();
            let scopes: HashSet<_> = [Scope::OfflineAccess].iter().cloned().collect();

            let (client_id, token) = client
                .register(
                    "client_name".to_string(),
                    scopes.clone(),
                    grants.clone(),
                    false,
                    false,
                    None,
                )
                .await?;

            let client = client.lookup(&client_id).await?.expect("failed to persist");

            assert!(token.is_none());
            assert_eq!(client.client_id, client_id);
            assert_eq!(client.client_name, "client_name");
            assert_eq!(client.scopes, scopes);
            assert_eq!(client.grants, grants);
            assert!(!client.loopback);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_update() -> Result<(), Box<dyn Error>> {
        let clients = clients()?;

        for client in clients.iter() {
            let grants: HashSet<_> = [GrantType::RefreshToken, GrantType::Password]
                .iter()
                .cloned()
                .collect();
            let scopes: HashSet<_> = [Scope::OfflineAccess].iter().cloned().collect();

            let (client_id, token) = client
                .register(
                    "client_name".to_string(),
                    Default::default(),
                    grants.clone(),
                    false,
                    false,
                    None,
                )
                .await?;

            client
                .update(
                    &client_id,
                    "client_name2".to_string(),
                    scopes.clone(),
                    Default::default(),
                    true,
                )
                .await?;

            let client = client.lookup(&client_id).await?.expect("failed to persist");

            assert!(token.is_none());
            assert_eq!(client.client_id, client_id);
            assert_eq!(client.client_name, "client_name2");
            assert_eq!(client.scopes, scopes);
            assert_eq!(client.grants, Default::default());
            assert!(client.loopback);
        }

        Ok(())
    }
}
