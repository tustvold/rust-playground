use crate::config::RabbitMQConfig;
use crate::mq::{Consumer, ConsumerDelegate, MQError, Message, MessageQueue};
use async_trait::async_trait;
use futures::stream::StreamExt;
use lapin::{
    options::*, types::FieldTable, BasicProperties, Channel, Connection, ConnectionProperties,
};
use log::error;
use std::error::Error;

const EXCHANGE: &str = "";
const QUEUE_NAME: &str = "index";

#[derive(Debug, Clone)]
pub struct RabbitMQConnection {
    connection: Connection,
}

impl RabbitMQConnection {
    pub fn new(config: &RabbitMQConfig) -> RabbitMQConnection {
        let connection = Connection::connect(&config.url, ConnectionProperties::default())
            .wait()
            .expect("Failed to connect to RabbitMQ");
        RabbitMQConnection { connection }
    }
}

#[derive(Debug, Clone)]
pub struct RabbitMQChannel {
    channel: Channel,
}

impl RabbitMQChannel {
    pub fn new(conn: &RabbitMQConnection) -> RabbitMQChannel {
        let channel: Channel = conn
            .connection
            .create_channel()
            .wait()
            .expect("Failed to create channel");

        channel
            .queue_declare(
                QUEUE_NAME,
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .wait()
            .expect("Failed to declare queue");

        channel
            .basic_qos(5, BasicQosOptions { global: false })
            .wait()
            .expect("Failed to set prefetch count");

        RabbitMQChannel { channel }
    }
}

#[async_trait(?Send)]
impl MessageQueue for RabbitMQChannel {
    async fn queue_index(&self, url: String) -> Result<(), MQError> {
        let encoded = serde_json::to_vec(&Message { url })?;

        self.channel
            .basic_publish(
                EXCHANGE,
                QUEUE_NAME,
                BasicPublishOptions::default(),
                encoded,
                BasicProperties::default(),
            )
            .await?;
        Ok(())
    }

    async fn consume(
        &self,
        delegate: Box<dyn ConsumerDelegate>,
    ) -> Result<Box<dyn Consumer>, Box<dyn Error>> {
        let consumer = self
            .channel
            .clone()
            .basic_consume(
                QUEUE_NAME,
                "test",
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;
        Ok(Box::new(ConsumerRabbitMQ {
            channel: self.channel.clone(),
            inner: consumer,
            delegate,
        }))
    }
}

struct ConsumerRabbitMQ {
    channel: Channel,
    inner: lapin::Consumer,
    delegate: Box<dyn ConsumerDelegate>,
}

#[async_trait(?Send)]
impl Consumer for ConsumerRabbitMQ {
    async fn block_on(&self) {
        self.inner
            .clone()
            .for_each_concurrent(None, |x| async move {
                match x {
                    Ok(delivery) => {
                        let tag = delivery.delivery_tag;

                        let value: Message = match serde_json::from_slice(&delivery.data) {
                            Ok(v) => v,
                            Err(e) => {
                                error!("Failed to deserialize message: {}", e);
                                return;
                            }
                        };

                        match self.delegate.consume(value).await {
                            Ok(_) => {
                                let ack = self
                                    .channel
                                    .basic_ack(tag, BasicAckOptions::default())
                                    .await;

                                if let Err(e) = ack {
                                    error!("Failed to ack message: {}", e)
                                }
                            }
                            Err(e) => {
                                error!("Delegate Error: {}", e);
                                let ack = self
                                    .channel
                                    .basic_nack(tag, BasicNackOptions::default())
                                    .await;

                                if let Err(e) = ack {
                                    error!("Failed to nack message: {}", e)
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("RabbitMQ Error: {}", e);
                    }
                }
            })
            .await
    }
}
