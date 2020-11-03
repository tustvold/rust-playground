use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::sync::Arc;

use async_trait::async_trait;
use rusoto_dynamodb::{AttributeValue, DeleteItemInput, DynamoDb, GetItemInput, UpdateItemInput};
use uuid::Uuid;

use credential::CredentialService;
use dynamo_util::IntoAttribute;
use telemetry::Measure;

use crate::dao::util::{dynamo_key, save_model};
use crate::dao::{error::DaoError, DaoConfig, UserDao};
use crate::model::{Scope, User, UserCredential};

lazy_static! {
    static ref CREATE_USER_MEASURE: Measure = Measure::new("dao", "user_dao_create_user");
    static ref GET_USER_MEASURE: Measure = Measure::new("dao", "user_dao_get_user");
    static ref CREATE_CREDENTIAL_MEASURE: Measure =
        Measure::new("dao", "user_dao_create_user_credential");
    static ref GET_CREDENTIAL_MEASURE: Measure = Measure::new("dao", "user_dao_get_credential");
    static ref DELETE_CREDENTIAL_MEASURE: Measure =
        Measure::new("dao", "user_dao_delete_credential");
    static ref VERIFY_MEASURE: Measure = Measure::new("dao", "user_dao_verify");
    static ref UPDATE_SCOPES_MEASURE: Measure = Measure::new("dao", "user_dao_update_scopes");
    static ref UPDATE_PASSWORD_MEASURE: Measure = Measure::new("dao", "user_dao_update_password");
}

pub struct UserDaoDynamo {
    table: String,
    client: Arc<dyn DynamoDb + Send + Sync>,
    credential: Arc<CredentialService>,
}

impl UserDaoDynamo {
    pub fn new(
        config: &DaoConfig,
        client: Arc<dyn DynamoDb + Send + Sync>,
        credential: Arc<CredentialService>,
    ) -> UserDaoDynamo {
        UserDaoDynamo {
            table: config.table.clone(),
            credential,
            client,
        }
    }

    pub async fn seed(&self, admin_pass: &str) -> Result<(), DaoError> {
        let user_id = "admin_id";
        let scopes = vec![Scope::Superuser].iter().cloned().collect();
        let res = self
            .create_credential("admin", user_id, admin_pass, scopes)
            .await;

        match res {
            Ok(_) => {
                println!("Created admin credential with password: {}", admin_pass);
                self.create_user("Administrator", Some(user_id.to_string()))
                    .await?;
                Ok(())
            }
            Err(DaoError::AlreadyExists) => {
                println!("Admin user already exists - not re-creating");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

#[async_trait]
impl UserDao for UserDaoDynamo {
    async fn create_user(&self, full_name: &str, opt: Option<String>) -> Result<String, DaoError> {
        CREATE_USER_MEASURE
            .stats(async move {
                let user_id = opt.unwrap_or_else(|| Uuid::new_v4().to_hyphenated().to_string());

                let user_record = User {
                    full_name: full_name.to_string(),
                    user_id: user_id.clone(),
                };

                save_model(
                    self.client.as_ref(),
                    self.table.clone(),
                    user_record.into(),
                    false,
                )
                .await?;

                Ok(user_id)
            })
            .await
    }

    async fn create_credential(
        &self,
        username: &str,
        user_id: &str,
        password: &str,
        scopes: HashSet<Scope>,
    ) -> Result<(), DaoError> {
        CREATE_CREDENTIAL_MEASURE
            .stats(async move {
                let credential = self
                    .credential
                    .derive(&username, password)
                    .await
                    .map_err(|_| DaoError::InvalidCredential)?;

                let user_credential = UserCredential {
                    username: username.to_string(),
                    user_id: user_id.to_string(),
                    credential,
                    scopes,
                };

                save_model(
                    self.client.as_ref(),
                    self.table.clone(),
                    user_credential.into(),
                    false,
                )
                .await
            })
            .await
    }

    async fn delete_credential(&self, username: &str) -> Result<(), DaoError> {
        DELETE_CREDENTIAL_MEASURE
            .stats(async move {
                self.client
                    .delete_item(DeleteItemInput {
                        table_name: self.table.clone(),
                        key: dynamo_key(UserCredential::pk(username)),
                        ..Default::default()
                    })
                    .await?;
                Ok(())
            })
            .await
    }

    async fn get_user(&self, user_id: &str) -> Result<Option<User>, DaoError> {
        GET_USER_MEASURE
            .stats(async move {
                let item = self
                    .client
                    .get_item(GetItemInput {
                        key: dynamo_key(User::pk(user_id)),
                        table_name: self.table.clone(),
                        ..Default::default()
                    })
                    .await?
                    .item;

                Ok(item.map(|x| x.try_into()).transpose()?)
            })
            .await
    }

    async fn get_credential(&self, username: &str) -> Result<Option<UserCredential>, DaoError> {
        GET_CREDENTIAL_MEASURE
            .stats(async move {
                let item = self
                    .client
                    .get_item(GetItemInput {
                        key: dynamo_key(UserCredential::pk(username)),
                        table_name: self.table.clone(),
                        ..Default::default()
                    })
                    .await?
                    .item;

                Ok(item.map(|x| x.try_into()).transpose()?)
            })
            .await
    }

    async fn verify(&self, username: &str, password: &str) -> Result<UserCredential, DaoError> {
        VERIFY_MEASURE
            .stats(async move {
                let cred = self
                    .get_credential(username)
                    .await?
                    .ok_or(DaoError::NotFound)?;

                self.credential
                    .verify(username, password, &cred.credential)
                    .await
                    .map_err(|_| DaoError::InvalidCredential)?;

                Ok(cred)
            })
            .await
    }

    async fn update_scopes(&self, username: &str, scopes: HashSet<Scope>) -> Result<(), DaoError> {
        UPDATE_SCOPES_MEASURE
            .stats(async move {
                if scopes.is_empty() {
                    self.client
                        .update_item(UpdateItemInput {
                            key: dynamo_key(UserCredential::pk(username)),
                            table_name: self.table.clone(),
                            update_expression: Some("REMOVE scopes".to_string()),
                            ..Default::default()
                        })
                        .await?;
                } else {
                    let mut map = HashMap::with_capacity(1);
                    map.insert(":scopes".to_string(), scopes.into_attribute());

                    self.client
                        .update_item(UpdateItemInput {
                            key: dynamo_key(UserCredential::pk(username)),
                            table_name: self.table.clone(),
                            update_expression: Some("SET scopes = :scopes".to_string()),
                            expression_attribute_values: Some(map),
                            ..Default::default()
                        })
                        .await?;
                }

                Ok(())
            })
            .await
    }

    async fn update_password(&self, username: &str, password: &str) -> Result<(), DaoError> {
        UPDATE_PASSWORD_MEASURE
            .stats(async move {
                let credential = self
                    .credential
                    .derive(username, password)
                    .await
                    .map_err(|_| DaoError::InvalidCredential)?;

                let mut map = HashMap::with_capacity(1);
                map.insert(
                    ":credential".to_string(),
                    AttributeValue {
                        b: Some(credential.into()),
                        ..Default::default()
                    },
                );

                self.client
                    .update_item(UpdateItemInput {
                        key: dynamo_key(UserCredential::pk(username)),
                        table_name: self.table.clone(),
                        update_expression: Some("SET credential = :credential".to_string()),
                        expression_attribute_values: Some(map),
                        ..Default::default()
                    })
                    .await?;

                Ok(())
            })
            .await
    }
}
