use std::collections::HashSet;
use std::convert::TryInto;
use std::sync::Arc;

use async_trait::async_trait;
use rusoto_dynamodb::{DynamoDb, GetItemInput};
use uuid::Uuid;

use credential::CredentialService;
use dynamo_util::UpdateBuilder;
use telemetry::Measure;

use crate::dao::error::DaoError;
use crate::dao::util::{dynamo_key, save_model};
use crate::dao::{ClientDao, DaoConfig};
use crate::model::{Client, GrantType, Scope};
use crate::service::token::TokenService;

lazy_static! {
    static ref REGISTER_MEASURE: Measure = Measure::new("dao", "client_dao_register");
    static ref UPDATE_MEASURE: Measure = Measure::new("dao", "client_dao_update");
    static ref LOOKUP_MEASURE: Measure = Measure::new("dao", "client_dao_lookup");
    static ref VERIFY_MEASURE: Measure = Measure::new("dao", "client_dao_verify");
}

pub struct ClientDaoDynamo {
    table: String,
    client: Arc<dyn DynamoDb + Send + Sync>,
    credential: Arc<CredentialService>,
    token: Arc<TokenService>,
}

impl ClientDaoDynamo {
    pub fn new(
        config: &DaoConfig,
        client: Arc<dyn DynamoDb + Send + Sync>,
        credential: Arc<CredentialService>,
        token: Arc<TokenService>,
    ) -> ClientDaoDynamo {
        ClientDaoDynamo {
            table: config.table.clone(),
            credential,
            token,
            client,
        }
    }

    pub async fn seed(&self) -> Result<(), DaoError> {
        let scopes = vec![Scope::Superuser].iter().cloned().collect();
        let grants = vec![GrantType::Password].iter().cloned().collect();

        match self
            .register(
                "loopback".to_string(),
                scopes,
                grants,
                false,
                true,
                Some("loopback".to_string()),
            )
            .await
        {
            Ok(_) => {
                println!("Created loopback client");
                Ok(())
            }
            Err(DaoError::AlreadyExists) => {
                println!("Looopback client already exists - not re-creating");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

#[async_trait]
impl ClientDao for ClientDaoDynamo {
    async fn register(
        &self,
        client_name: String,
        scopes: HashSet<Scope>,
        grants: HashSet<GrantType>,
        generate_credential: bool,
        loopback: bool,
        client_id: Option<String>,
    ) -> Result<(String, Option<String>), DaoError> {
        REGISTER_MEASURE
            .stats(async move {
                let client_id =
                    client_id.unwrap_or_else(|| Uuid::new_v4().to_hyphenated().to_string());

                let (token_opt, credential) = if generate_credential {
                    let token = self.token.token()?;

                    let hashed_token = self
                        .credential
                        .derive(&client_id, &token)
                        .await
                        .map_err(|_| DaoError::InvalidCredential)?;
                    (Some(token), Some(hashed_token))
                } else {
                    (None, None)
                };

                let item = Client {
                    client_id: client_id.clone(),
                    client_name,
                    scopes,
                    grants,
                    credential,
                    loopback,
                };

                save_model(self.client.as_ref(), self.table.clone(), item.into(), false).await?;
                Ok((client_id, token_opt))
            })
            .await
    }

    async fn update(
        &self,
        client_id: &str,
        client_name: String,
        scopes: HashSet<Scope>,
        grants: HashSet<GrantType>,
        loopback: bool,
    ) -> Result<(), DaoError> {
        UPDATE_MEASURE
            .stats(async move {
                let mut builder = UpdateBuilder::new(4)
                    .value("client_name", client_name)
                    .value("loopback", loopback);

                if grants.is_empty() {
                    builder = builder.remove("grants");
                } else {
                    builder = builder.value("grants", grants);
                }

                if scopes.is_empty() {
                    builder = builder.remove("scopes");
                } else {
                    builder = builder.value("scopes", scopes);
                }

                let item = builder.build(dynamo_key(Client::pk(client_id)), self.table.clone());

                self.client.update_item(item).await?;
                Ok(())
            })
            .await
    }

    async fn lookup(&self, client_id: &str) -> Result<Option<Client>, DaoError> {
        LOOKUP_MEASURE
            .stats(async move {
                let item = self
                    .client
                    .get_item(GetItemInput {
                        key: dynamo_key(Client::pk(client_id)),
                        table_name: self.table.clone(),
                        ..Default::default()
                    })
                    .await?
                    .item;

                Ok(item.map(|x| x.try_into()).transpose()?)
            })
            .await
    }

    async fn verify(
        &self,
        client_id: &str,
        token: &str,
        hashed_token: &[u8],
    ) -> Result<(), DaoError> {
        VERIFY_MEASURE
            .stats(async move {
                self.credential
                    .verify(&client_id, &token, hashed_token)
                    .await
                    .map_err(|_| DaoError::InvalidCredential)?;

                Ok(())
            })
            .await
    }
}
