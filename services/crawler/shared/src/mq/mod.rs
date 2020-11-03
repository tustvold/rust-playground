use async_trait::async_trait;
use derive_more::Display;

mod rabbitmq;

pub use rabbitmq::{RabbitMQChannel, RabbitMQConnection};
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Debug, Display)]
pub struct MQError {
    message: String,
}
impl std::error::Error for MQError {}

impl From<lapin::Error> for MQError {
    fn from(e: lapin::Error) -> Self {
        MQError {
            message: e.to_string(),
        }
    }
}

impl From<serde_json::Error> for MQError {
    fn from(e: serde_json::Error) -> Self {
        MQError {
            message: e.to_string(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Message {
    pub url: String,
}

#[async_trait(?Send)]
pub trait MessageQueue {
    async fn queue_index(&self, url: String) -> Result<(), MQError>;

    async fn consume(
        &self,
        delegate: Box<dyn ConsumerDelegate>,
    ) -> Result<Box<dyn Consumer>, Box<dyn Error>>;
}

#[async_trait(?Send)]
pub trait Consumer {
    async fn block_on(&self);
}

#[async_trait(?Send)]
pub trait ConsumerDelegate {
    async fn consume(&self, message: Message) -> Result<(), Box<dyn Error>>;
}
