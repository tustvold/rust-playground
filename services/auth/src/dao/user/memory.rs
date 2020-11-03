use std::collections::hash_map::RandomState;
use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::dao::{DaoError, UserDao};
use crate::model::{Scope, User, UserCredential};

pub struct UserDaoMemory {
    users: Mutex<HashMap<String, User>>,
    user_credentials: Mutex<HashMap<String, UserCredential>>,
}

impl UserDaoMemory {
    #[allow(dead_code)]
    pub fn new() -> UserDaoMemory {
        UserDaoMemory {
            users: Mutex::new(Default::default()),
            user_credentials: Mutex::new(Default::default()),
        }
    }
}

#[async_trait]
impl UserDao for UserDaoMemory {
    async fn create_user(
        &self,
        full_name: &str,
        user_id: Option<String>,
    ) -> Result<String, DaoError> {
        let user_id = user_id.unwrap_or_else(|| Uuid::new_v4().to_hyphenated().to_string());
        let mut data = self.users.lock().await;
        if data.contains_key(&user_id) {
            return Err(DaoError::AlreadyExists);
        }
        data.insert(
            user_id.clone(),
            User {
                full_name: full_name.to_string(),
                user_id: user_id.clone(),
            },
        );
        Ok(user_id)
    }

    async fn create_credential(
        &self,
        username: &str,
        user_id: &str,
        password: &str,
        scopes: HashSet<Scope, RandomState>,
    ) -> Result<(), DaoError> {
        let mut data = self.user_credentials.lock().await;
        if data.contains_key(username) {
            return Err(DaoError::AlreadyExists);
        }

        data.insert(
            username.to_string(),
            UserCredential {
                username: username.to_string(),
                user_id: user_id.to_string(),
                credential: password.as_bytes().to_vec(),
                scopes,
            },
        );
        Ok(())
    }

    async fn delete_credential(&self, username: &str) -> Result<(), DaoError> {
        let mut data = self.user_credentials.lock().await;
        data.remove(username).ok_or(DaoError::NotFound)?;
        Ok(())
    }

    async fn get_user(&self, user_id: &str) -> Result<Option<User>, DaoError> {
        let data = self.users.lock().await;
        Ok(data.get(user_id).cloned())
    }

    async fn get_credential(&self, username: &str) -> Result<Option<UserCredential>, DaoError> {
        let data = self.user_credentials.lock().await;
        Ok(data.get(username).cloned())
    }

    async fn verify(&self, username: &str, password: &str) -> Result<UserCredential, DaoError> {
        let cred = self
            .get_credential(username)
            .await?
            .ok_or(DaoError::NotFound)?;

        let expected = String::from_utf8(cred.credential.clone())
            .map_err(|e| DaoError::InternalError(e.to_string()))?;

        if expected == password {
            Ok(cred)
        } else {
            Err(DaoError::InvalidCredential)
        }
    }

    async fn update_scopes(
        &self,
        username: &str,
        scopes: HashSet<Scope, RandomState>,
    ) -> Result<(), DaoError> {
        let mut data = self.user_credentials.lock().await;
        let cred = data.get_mut(username).ok_or(DaoError::NotFound)?;
        cred.scopes = scopes;
        Ok(())
    }

    async fn update_password(&self, username: &str, password: &str) -> Result<(), DaoError> {
        let mut data = self.user_credentials.lock().await;
        let cred = data.get_mut(username).ok_or(DaoError::NotFound)?;
        cred.credential = password.as_bytes().to_vec();
        Ok(())
    }
}
