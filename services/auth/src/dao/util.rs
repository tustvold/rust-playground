use std::collections::HashMap;

use rusoto_core::RusotoError;
use rusoto_dynamodb::{AttributeValue, DynamoDb, PutItemError, PutItemInput};

use crate::dao::DaoError;

pub fn dynamo_key(pk: String) -> HashMap<String, AttributeValue> {
    let mut key = HashMap::new();
    key.insert(
        "pk".to_string(),
        AttributeValue {
            s: Some(pk),
            ..Default::default()
        },
    );
    key
}

pub async fn save_model(
    client: &(dyn DynamoDb + Send + Sync),
    table_name: String,
    item: HashMap<String, AttributeValue>,
    exists: bool,
) -> Result<(), DaoError> {
    let condition = if exists {
        "attribute_exists(pk)"
    } else {
        "attribute_not_exists(pk)"
    };

    match client
        .put_item(PutItemInput {
            item,
            table_name,
            condition_expression: Some(condition.to_string()),
            ..Default::default()
        })
        .await
    {
        Ok(_) => Ok(()),
        Err(RusotoError::Service(PutItemError::ConditionalCheckFailed(_))) => {
            if exists {
                Err(DaoError::NotFound)
            } else {
                Err(DaoError::AlreadyExists)
            }
        }
        Err(e) => Err(DaoError::from(e)),
    }
}
