use async_trait::async_trait;
use crawler::CrawlError;
use log::{error, info};
use reqwest::Url;
use shared::dao::{LinkDao, LinkDaoDynamo};
use shared::mq::*;
use std::collections::HashSet;
use std::error::Error;

mod crawler;
mod decoder;
mod parser;

struct Delegate {
    dao: LinkDaoDynamo,
    channel: RabbitMQChannel,
}

#[async_trait(?Send)]
impl ConsumerDelegate for Delegate {
    async fn consume(&self, message: Message) -> Result<(), Box<dyn Error>> {
        println!("{}", &message.url);
        if self.dao.get_links(&message.url).await?.is_some() {
            info!("Already indexed {}", &message.url);
        } else {
            let base = Url::parse(&message.url)?;

            let urls = match crawler::crawl(&base).await {
                Ok(urls) => urls,
                Err(CrawlError::NonHtmlContent) => Default::default(),
                Err(CrawlError::DecodeError) => {
                    error!("Error decoding url content: {}", message.url);
                    Default::default()
                }
                Err(e) => return Err(e.into()),
            };

            let filtered_urls: HashSet<String> = urls
                .iter()
                .filter(|x| x.origin() == base.origin())
                .map(|x| x.to_string())
                .collect();

            let links = urls.iter().map(|x| x.to_string()).collect();
            self.dao.set_links(message.url, links).await?;

            let crawled = self.dao.get_multiple(&filtered_urls).await?;
            for next in filtered_urls.difference(&crawled) {
                println!("{}", next);
                self.channel.queue_index(next.clone()).await?;
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let config = shared::config::Config::from_env().unwrap();
    let connection = RabbitMQConnection::new(&config.rabbit);
    let send = RabbitMQChannel::new(&connection);
    let recv = RabbitMQChannel::new(&connection);
    let dao = LinkDaoDynamo::new(&config.dynamo);

    let delegate = Box::new(Delegate { dao, channel: send });

    let res = recv.consume(delegate).await?;
    res.block_on().await;

    Ok(())
}
