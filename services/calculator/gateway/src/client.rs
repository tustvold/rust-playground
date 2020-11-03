use crate::error::ApiError;
use calculator_client::{ComputeRequest, ComputeValue};

pub struct CalculatorClient {
    post_url: String,
    client: reqwest::Client,
}

impl CalculatorClient {
    pub fn new(client: reqwest::Client, upstream: String) -> CalculatorClient {
        CalculatorClient {
            post_url: format!("{}/api/v1/compute", upstream),
            client,
        }
    }

    pub async fn compute(
        &self,
        request: &ComputeRequest,
        authorization: String,
    ) -> Result<ComputeValue, ApiError> {
        self.client
            .post(&self.post_url)
            .header("Authorization", authorization)
            .json(request)
            .send()
            .await?
            .json::<ComputeValue>()
            .await
            .map_err(ApiError::from)
    }
}
