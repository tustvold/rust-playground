use std::collections::HashSet;
use std::convert::TryInto;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusoto_dynamodb::{DeleteItemInput, DynamoDb};

use credential::CredentialService;
use telemetry::Measure;

use crate::dao::error::DaoError;
use crate::dao::util::{dynamo_key, save_model};
use crate::dao::{DaoConfig, RenewalTokenDao};
use crate::model::{RenewalToken, Scope};
use crate::service::token::TokenService;

lazy_static! {
    static ref GENERATE_MEASURE: Measure = Measure::new("dao", "renewal_token_dao_generate");
    static ref CONSUME_MEASURE: Measure = Measure::new("dao", "renewal_token_dao_consume");
}

pub struct RenewalTokenDaoDynamo {
    table: String,
    client: Arc<dyn DynamoDb + Send + Sync>,
    credential: Arc<CredentialService>,
    token: Arc<TokenService>,
}

impl RenewalTokenDaoDynamo {
    pub fn new(
        config: &DaoConfig,
        client: Arc<dyn DynamoDb + Send + Sync>,
        credential: Arc<CredentialService>,
        token: Arc<TokenService>,
    ) -> RenewalTokenDaoDynamo {
        RenewalTokenDaoDynamo {
            table: config.table.clone(),
            credential,
            client,
            token,
        }
    }

    // Returns a hash of the token - this is not ideal as client_id is potentially
    // shared between lots of users but it is better than nothing
    async fn hash_token(&self, client_id: &str, token: &str) -> Result<Vec<u8>, DaoError> {
        self.credential
            .derive(client_id, &token)
            .await
            .map_err(|_| DaoError::InvalidCredential)
    }
}

#[async_trait]
impl RenewalTokenDao for RenewalTokenDaoDynamo {
    async fn generate(
        &self,
        subject: &str,
        client_id: &str,
        device_name: &str,
        scopes: HashSet<Scope>,
        expiry: DateTime<Utc>,
    ) -> Result<String, DaoError> {
        GENERATE_MEASURE
            .stats(async move {
                let token = self.token.token()?;

                let hashed_token = self.hash_token(client_id, &token).await?;

                let item = RenewalToken {
                    client_id: client_id.to_string(),
                    subject: subject.to_string(),
                    device_name: device_name.to_string(),
                    expiry,
                    scopes,
                    hashed_token,
                };

                save_model(self.client.as_ref(), self.table.clone(), item.into(), false).await?;
                Ok(token)
            })
            .await
    }

    async fn consume(&self, client_id: &str, token: &str) -> Result<RenewalToken, DaoError> {
        CONSUME_MEASURE
            .stats(async move {
                let hashed_token = self.hash_token(client_id, &token).await?;

                let item = self
                    .client
                    .delete_item(DeleteItemInput {
                        key: dynamo_key(RenewalToken::pk(client_id, &hashed_token)),
                        table_name: self.table.clone(),
                        return_values: Some("ALL_OLD".to_string()),
                        ..Default::default()
                    })
                    .await?
                    .attributes
                    .ok_or(DaoError::InvalidCredential)?;

                let parsed: RenewalToken = item.try_into()?;

                let now = Utc::now();
                if parsed.expiry < now {
                    return Err(DaoError::ExpiredCredential);
                }

                Ok(parsed)
            })
            .await
    }
}
