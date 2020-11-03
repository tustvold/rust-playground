use std::error::Error;

use rusoto_core::RusotoError;

use async_trait::async_trait;
use derive_more::Display;
use std::collections::HashSet;

pub use dynamo::LinkDaoDynamo;

mod dynamo;

#[derive(Debug, Display)]
pub struct LinkDaoError {
    message: String,
}

impl LinkDaoError {
    fn new(message: String) -> LinkDaoError {
        LinkDaoError { message }
    }
}

impl std::error::Error for LinkDaoError {}

impl From<serde_dynamodb::error::Error> for LinkDaoError {
    fn from(e: serde_dynamodb::error::Error) -> Self {
        LinkDaoError { message: e.message }
    }
}

impl<E: Error + 'static> From<RusotoError<E>> for LinkDaoError {
    fn from(e: RusotoError<E>) -> Self {
        LinkDaoError {
            message: e.to_string(),
        }
    }
}

#[async_trait(?Send)]
pub trait LinkDao {
    async fn get_links(&self, url: &str) -> Result<Option<HashSet<String>>, LinkDaoError>;

    async fn get_multiple(&self, urls: &HashSet<String>) -> Result<HashSet<String>, LinkDaoError>;

    async fn set_links(&self, url: String, links: HashSet<String>) -> Result<(), LinkDaoError>;
}
