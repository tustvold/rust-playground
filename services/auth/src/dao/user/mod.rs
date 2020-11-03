use std::collections::HashSet;

use async_trait::async_trait;

pub use dynamo::UserDaoDynamo;
pub use memory::UserDaoMemory;

use crate::dao::error::DaoError;
use crate::model::{Scope, User, UserCredential};

mod dynamo;
mod memory;

#[async_trait]
pub trait UserDao: Sync + Send {
    async fn create_user(
        &self,
        full_name: &str,
        user_id: Option<String>,
    ) -> Result<String, DaoError>;

    async fn create_credential(
        &self,
        username: &str,
        user_id: &str,
        password: &str,
        scopes: HashSet<Scope>,
    ) -> Result<(), DaoError>;

    async fn delete_credential(&self, username: &str) -> Result<(), DaoError>;

    async fn get_user(&self, user_id: &str) -> Result<Option<User>, DaoError>;

    async fn get_credential(&self, username: &str) -> Result<Option<UserCredential>, DaoError>;

    async fn verify(&self, username: &str, password: &str) -> Result<UserCredential, DaoError>;

    async fn update_scopes(&self, username: &str, scopes: HashSet<Scope>) -> Result<(), DaoError>;

    async fn update_password(&self, username: &str, password: &str) -> Result<(), DaoError>;
}

#[cfg(test)]
mod test {
    use std::error::Error;
    use std::sync::Arc;

    use credential::CredentialService;

    use super::*;

    fn clients() -> Result<Vec<Box<dyn UserDao>>, Box<dyn Error>> {
        let figment = rocket::Config::figment();
        let config: crate::config::Config = figment.extract().unwrap();
        let client = Arc::new(config.dao.dynamo_client());
        let credential = Arc::new(CredentialService::test()?);

        Ok(vec![
            Box::new(UserDaoDynamo::new(&config.dao, client, credential)),
            Box::new(UserDaoMemory::new()),
        ])
    }

    #[tokio::test]
    async fn test_create_user() -> Result<(), Box<dyn Error>> {
        let clients = clients()?;

        for client in clients.iter() {
            let user_id = client.create_user("asdf", None).await?;

            let user = client.get_user(&user_id).await?.expect("not persisted");

            assert_eq!(user.user_id, user_id);
            assert_eq!(user.full_name, "asdf")
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_create_user_credential() -> Result<(), Box<dyn Error>> {
        let clients = clients()?;

        for client in clients.iter() {
            let scopes: HashSet<_> = [Scope::OfflineAccess].iter().cloned().collect();
            let _ = client
                .delete_credential("test_create_user_credential")
                .await;

            client
                .create_credential(
                    "test_create_user_credential",
                    "test_user_id",
                    "password123",
                    scopes.clone(),
                )
                .await?;

            let credential = client
                .get_credential("test_create_user_credential")
                .await?
                .expect("not persisted");

            assert_eq!(credential.user_id, "test_user_id");
            assert_eq!(credential.scopes, scopes);
            assert_eq!(credential.username, "test_create_user_credential");
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_credentials() -> Result<(), Box<dyn Error>> {
        let clients = clients()?;

        for client in clients.iter() {
            let scopes: HashSet<_> = [Scope::OfflineAccess].iter().cloned().collect();
            let _ = client.delete_credential("test_credentials").await;

            client
                .create_credential(
                    "test_credentials",
                    "test_user_id",
                    "password123",
                    scopes.clone(),
                )
                .await?;

            let cred = client.verify("test_credentials", "password123").await?;
            assert_eq!(cred.user_id, "test_user_id");

            match client.verify("test_credentials", "asdf").await {
                Err(DaoError::InvalidCredential) => (),
                _ => panic!(),
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_duplicate() -> Result<(), Box<dyn Error>> {
        let clients = clients()?;

        for client in clients.iter() {
            let scopes: HashSet<_> = [Scope::OfflineAccess].iter().cloned().collect();
            let _ = client.delete_credential("test_duplicate").await;
            client
                .create_credential(
                    "test_duplicate",
                    "test_user_id",
                    "password123",
                    scopes.clone(),
                )
                .await?;

            match client
                .create_credential(
                    "test_duplicate",
                    "test_user_id",
                    "password123",
                    scopes.clone(),
                )
                .await
            {
                Err(DaoError::AlreadyExists) => (),
                _ => panic!(),
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_change_password() -> Result<(), Box<dyn Error>> {
        let clients = clients()?;

        for client in clients.iter() {
            let scopes: HashSet<_> = [Scope::OfflineAccess].iter().cloned().collect();
            let _ = client.delete_credential("test_change_password").await;
            client
                .create_credential(
                    "test_change_password",
                    "test_user_id",
                    "password123",
                    scopes.clone(),
                )
                .await?;

            client
                .update_password("test_change_password", "new_password")
                .await?;

            client
                .verify("test_change_password", "new_password")
                .await?;

            match client.verify("test_change_password", "password123").await {
                Err(DaoError::InvalidCredential) => (),
                _ => panic!(),
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_change_scopes() -> Result<(), Box<dyn Error>> {
        let clients = clients()?;

        for client in clients.iter() {
            let scopes: HashSet<_> = [Scope::OfflineAccess].iter().cloned().collect();
            let _ = client.delete_credential("test_change_scopes").await;
            client
                .create_credential(
                    "test_change_scopes",
                    "test_user_id",
                    "password123",
                    scopes.clone(),
                )
                .await?;

            client
                .update_scopes("test_change_scopes", Default::default())
                .await?;

            let cred1 = client.get_credential("test_change_scopes").await?.unwrap();

            client
                .update_scopes("test_change_scopes", scopes.clone())
                .await?;

            let cred2 = client.get_credential("test_change_scopes").await?.unwrap();

            assert!(cred1.scopes.is_empty());
            assert_eq!(cred2.scopes, scopes);
        }
        Ok(())
    }
}
