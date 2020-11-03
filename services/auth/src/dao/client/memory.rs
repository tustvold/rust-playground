use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::dao::{ClientDao, DaoError};
use crate::model::{Client, GrantType, Scope};
use crate::service::token::TokenService;

pub struct ClientDaoMemory {
    data: Mutex<HashMap<String, Client>>,
    token: Arc<TokenService>,
}

impl ClientDaoMemory {
    #[allow(dead_code)]
    pub fn new(token: Arc<TokenService>) -> ClientDaoMemory {
        ClientDaoMemory {
            data: Mutex::new(Default::default()),
            token,
        }
    }
}

#[async_trait]
impl ClientDao for ClientDaoMemory {
    async fn register(
        &self,
        client_name: String,
        scopes: HashSet<Scope>,
        grants: HashSet<GrantType>,
        generate_credential: bool,
        loopback: bool,
        client_id: Option<String>,
    ) -> Result<(String, Option<String>), DaoError> {
        let client_id = client_id.unwrap_or_else(|| Uuid::new_v4().to_hyphenated().to_string());
        let (token_opt, credential) = if generate_credential {
            let token = self.token.token()?;
            let credential = token.as_bytes().to_vec();
            (Some(token), Some(credential))
        } else {
            (None, None)
        };

        let mut data = self.data.lock().await;
        if data.contains_key(&client_id) {
            return Err(DaoError::AlreadyExists);
        }

        data.insert(
            client_id.clone(),
            Client {
                client_id: client_id.clone(),
                client_name,
                credential,
                scopes,
                grants,
                loopback,
            },
        );

        Ok((client_id, token_opt))
    }

    async fn update(
        &self,
        client_id: &str,
        client_name: String,
        scopes: HashSet<Scope>,
        grants: HashSet<GrantType>,
        loopback: bool,
    ) -> Result<(), DaoError> {
        let mut data = self.data.lock().await;
        let client = data.get_mut(client_id).ok_or(DaoError::NotFound)?;

        client.client_name = client_name;
        client.scopes = scopes;
        client.grants = grants;
        client.loopback = loopback;

        Ok(())
    }

    async fn lookup(&self, client_id: &str) -> Result<Option<Client>, DaoError> {
        let data = self.data.lock().await;
        Ok(data.get(client_id).cloned())
    }

    async fn verify(&self, _: &str, token: &str, hashed_token: &[u8]) -> Result<(), DaoError> {
        let expected =
            String::from_utf8(hashed_token.to_vec()).map_err(|_| DaoError::InvalidCredential)?;
        if expected == token {
            Ok(())
        } else {
            Err(DaoError::InvalidCredential)
        }
    }
}
