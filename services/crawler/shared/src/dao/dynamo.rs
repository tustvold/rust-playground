use std::collections::{HashMap, HashSet};

use rusoto_dynamodb::{
    AttributeValue, BatchGetItemInput, DynamoDb, DynamoDbClient, GetItemInput, KeysAndAttributes,
    PutItemInput,
};

use async_trait::async_trait;

use crate::config::DynamoConfig;
use crate::dao::{LinkDao, LinkDaoError};
use futures::future::join_all;
use serde::{Deserialize, Serialize};

const TABLE_NAME: &str = "crawler";
const PRIMARY_KEY: &str = "Url";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct CrawlEntry {
    url: String,
    links: HashSet<String>,
}

pub struct LinkDaoDynamo {
    client: DynamoDbClient,
}

impl LinkDaoDynamo {
    pub fn new(config: &DynamoConfig) -> LinkDaoDynamo {
        let client = config.dynamo_client();
        LinkDaoDynamo { client }
    }

    async fn get_batch(
        &self,
        keys: &[HashMap<String, AttributeValue>],
    ) -> Result<Vec<String>, LinkDaoError> {
        let request_items = [(
            String::from(TABLE_NAME),
            KeysAndAttributes {
                keys: keys.into(),
                ..Default::default()
            },
        )]
        .iter()
        .cloned()
        .collect();

        let res = self
            .client
            .batch_get_item(BatchGetItemInput {
                request_items,
                ..Default::default()
            })
            .await?;

        match res.responses {
            Some(responses) => match responses.get(TABLE_NAME) {
                Some(items) => items
                    .iter()
                    .map(|item| {
                        let entry: CrawlEntry = serde_dynamodb::from_hashmap(item.clone())?;
                        Ok(entry.url)
                    })
                    .collect::<Result<Vec<_>, _>>(),
                None => Err(LinkDaoError::new("Response missing table name".to_string())),
            },
            None => Ok(Default::default()),
        }
    }
}

fn get_key(url: &str) -> HashMap<String, AttributeValue> {
    [(
        String::from(PRIMARY_KEY),
        AttributeValue {
            s: Some(String::from(url)),
            ..Default::default()
        },
    )]
    .iter()
    .cloned()
    .collect()
}

#[async_trait(? Send)]
impl LinkDao for LinkDaoDynamo {
    async fn get_links(&self, url: &str) -> Result<Option<HashSet<String>>, LinkDaoError> {
        self.client
            .get_item(GetItemInput {
                key: get_key(url),
                table_name: String::from(TABLE_NAME),
                ..Default::default()
            })
            .await?
            .item
            .map_or(Ok(None), |item| {
                let entry: CrawlEntry = serde_dynamodb::from_hashmap(item)?;
                Ok(Some(entry.links))
            })
    }

    async fn get_multiple(&self, urls: &HashSet<String>) -> Result<HashSet<String>, LinkDaoError> {
        let keys: Vec<HashMap<String, AttributeValue>> = urls.iter().map(|k| get_key(k)).collect();
        let results = join_all(keys.chunks(100).map(|chunk| self.get_batch(chunk))).await;

        let mut ret: HashSet<String> = HashSet::with_capacity(urls.len());
        for r in results {
            match r {
                Ok(set) => ret.extend(set),
                Err(e) => return Err(e),
            }
        }
        Ok(ret)
    }

    async fn set_links(&self, url: String, links: HashSet<String>) -> Result<(), LinkDaoError> {
        let entry = CrawlEntry { url, links };
        self.client
            .put_item(PutItemInput {
                item: serde_dynamodb::to_hashmap(&entry)?,
                table_name: String::from(TABLE_NAME),
                ..Default::default()
            })
            .await?;
        Ok(())
    }
}
