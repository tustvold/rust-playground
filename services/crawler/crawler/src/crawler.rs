use derive_more::Display;
use std::collections::HashSet;
use std::error::Error;
use std::time::Duration;

use reqwest::Response;
use url::Url;

use crate::decoder::streaming_decode;
use crate::parser::Parser;

#[derive(Debug, Display, PartialEq)]
pub enum CrawlError {
    #[display(fmt = "Non text response")]
    NonHtmlContent,
    #[display(fmt = "Error encountered decoding data")]
    DecodeError,
    #[display(fmt = "Error making request")]
    RequestError(String),
}
impl Error for CrawlError {}

impl From<reqwest::Error> for CrawlError {
    fn from(r: reqwest::Error) -> Self {
        CrawlError::RequestError(r.to_string())
    }
}

pub async fn crawl(base: &Url) -> Result<HashSet<Url>, CrawlError> {
    let client = reqwest::ClientBuilder::new()
        .connect_timeout(Duration::from_secs(5))
        .build()
        .expect("Failed to build client");

    let mut parser = Parser::new(base.clone());
    let mut res: Response = client.get(base.as_str()).send().await?;
    streaming_decode(&mut res, |x| parser.feed(x)).await?;

    Ok(parser.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_crawl() -> Result<(), Box<dyn Error>> {
        let url = Url::parse("https://accounts.google.com/ServiceLogin?hl=en&passive=true&continue=https://www.google.co.uk/")?;
        let res = crawl(&url).await;
        assert!(res.is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn test_nonhtml() -> Result<(), Box<dyn Error>> {
        let url = Url::parse("https://monzo.com/documents/pillar_3_2019.pdf")?;
        let res = crawl(&url).await;

        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), CrawlError::NonHtmlContent);
        Ok(())
    }
}
