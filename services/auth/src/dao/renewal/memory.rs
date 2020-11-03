use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::Mutex;

use crate::dao::error::DaoError;
use crate::dao::renewal::RenewalTokenDao;
use crate::model::{RenewalToken, Scope};
use crate::service::token::TokenService;

pub struct RenewalTokenDaoMemory {
    data: Mutex<HashMap<String, RenewalToken>>,
    token: Arc<TokenService>,
}

impl RenewalTokenDaoMemory {
    #[allow(dead_code)]
    pub fn new(token: Arc<TokenService>) -> RenewalTokenDaoMemory {
        RenewalTokenDaoMemory {
            data: Mutex::new(Default::default()),
            token,
        }
    }
}

#[async_trait]
impl RenewalTokenDao for RenewalTokenDaoMemory {
    async fn generate(
        &self,
        subject: &str,
        client_id: &str,
        device_name: &str,
        scopes: HashSet<Scope>,
        expiry: DateTime<Utc>,
    ) -> Result<String, DaoError> {
        let token = self.token.token()?;

        let key = [client_id, &token].join("#");

        let mut data = self.data.lock().await;
        if data.contains_key(&key) {
            return Err(DaoError::AlreadyExists);
        }

        data.insert(
            key,
            RenewalToken {
                client_id: client_id.to_string(),
                subject: subject.to_string(),
                device_name: device_name.to_string(),
                hashed_token: vec![],
                expiry,
                scopes,
            },
        );

        Ok(token)
    }

    async fn consume(&self, client_id: &str, token: &str) -> Result<RenewalToken, DaoError> {
        let key = [client_id, &token].join("#");
        let mut data = self.data.lock().await;
        let parsed = data.remove(&key).ok_or(DaoError::InvalidCredential)?;

        let now = Utc::now();
        if parsed.expiry < now {
            return Err(DaoError::ExpiredCredential);
        }
        Ok(parsed)
    }
}
