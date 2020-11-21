use futures::StreamExt;
use rusoto_core::credential::StaticProvider;
use rusoto_kinesis::KinesisClient;
use tokio::sync::mpsc;
use tokio::task::{JoinError, JoinHandle};
use tokio::time::Duration;
use tracing::info;

use rusoto_util::{parse_region, CustomChainProvider};
use stream::{BatchStreamExt, LimitedStreamExt};

use crate::aggregator::RecordAggregator;
use crate::producer::{Producer, RecordBatcher, RecordLimiter};
use crate::sink::{ErrorHandler, KinesisSink};
use crate::topology::TopologyService;

mod aggregator;
mod intern;
pub mod producer;
mod shutdown;
mod sink;
mod topology;

const BYTES_PER_MB: usize = 1024 * 1024;

pub struct PipelineHandler {
    worker_handle: JoinHandle<()>,
    worker_shutdown: shutdown::Sender,
}

impl PipelineHandler {
    pub async fn shutdown(self) -> Result<(), JoinError> {
        self.worker_shutdown.shutdown();
        self.worker_handle.await
    }
}

struct ReducerConfig {
    max_records: usize,
    max_bytes: usize,
    max_wait: Duration,
}

pub struct PipelineBuilder {
    region: String,
    stream: String,
    endpoint: Option<String>,
    rps_per_shard: u64,
    bps_per_shard: u64,

    batch_config: ReducerConfig,
    aggregator_config: ReducerConfig,

    retry_backoff: Duration,
    local: bool,
}

impl PipelineBuilder {
    /// Creates a new producer pipeline
    pub fn new(region: String, stream: String) -> PipelineBuilder {
        PipelineBuilder {
            region,
            stream,
            endpoint: None,
            local: false,
            rps_per_shard: 1500,
            bps_per_shard: 7 * BYTES_PER_MB as u64,
            retry_backoff: Duration::from_secs(1),
            aggregator_config: ReducerConfig {
                max_records: 4294967295,
                max_bytes: 51200,
                max_wait: Duration::from_secs(1),
            },
            batch_config: ReducerConfig {
                max_records: 500,
                max_bytes: 4 * BYTES_PER_MB,
                max_wait: Duration::from_millis(500),
            },
        }
    }

    /// Use local kinesalite endpoint
    pub fn local(&mut self) -> &mut Self {
        self.local = true;
        self
    }

    /// Override endpoint
    pub fn endpoint(&mut self, endpoint: String) -> &mut Self {
        self.endpoint = Some(endpoint);
        self
    }

    /// Set the rate per shard rate limits
    ///
    /// Note: Records larger than bytes per second will be dropped - set the aggregation size accordingly
    pub fn shard_rate_limit(
        &mut self,
        records_per_second: u64,
        bytes_per_second: u64,
    ) -> &mut Self {
        self.bps_per_shard = bytes_per_second;
        self.rps_per_shard = records_per_second;
        self
    }

    /// Configures the backoff delay following a PutRecords error
    pub fn retry_backoff(&mut self, backoff: Duration) -> &mut Self {
        self.retry_backoff = backoff;
        self
    }

    /// Configures the how the pipeline should batch records to the PutRecords API
    pub fn batch(&mut self, max_bytes: usize, max_records: usize, max_wait: Duration) -> &mut Self {
        self.batch_config = ReducerConfig {
            max_records,
            max_bytes,
            max_wait,
        };
        self
    }

    /// Configures the how the pipeline should aggregate records for the same shard together
    pub fn aggregate(
        &mut self,
        max_bytes: usize,
        max_records: usize,
        max_wait: Duration,
    ) -> &mut Self {
        self.aggregator_config = ReducerConfig {
            max_records,
            max_bytes,
            max_wait,
        };
        self
    }

    pub fn build(self) -> (Producer, PipelineHandler) {
        let client = kinesis_client(self.region, self.endpoint, self.local);

        let (sender, receiver) = mpsc::channel(1000);
        let (shutdown_tx, shutdown_rx) = shutdown::channel();

        let (topology, topology_worker) =
            TopologyService::new(client.clone(), self.stream.clone(), shutdown_rx.clone());

        let (retry, retry_worker) = ErrorHandler::new(
            sender.clone(),
            topology.clone(),
            self.retry_backoff,
            shutdown_rx.clone(),
        );
        let kinesis_sink = KinesisSink::new(client, self.stream, retry);

        let rps_per_shard = self.rps_per_shard;
        let bps_per_shard = self.bps_per_shard;
        let batch_config = self.batch_config;
        let aggregator_config = self.aggregator_config;

        let worker_handle = tokio::spawn(Box::pin(async move {
            let fut1 = receiver
                .take_until(shutdown_rx)
                .then(|mut record| {
                    let mut topology = topology.clone();
                    async move {
                        record.predicted_shard_id =
                            Some(topology.lookup_shard(record.hash_key()).await);
                        record
                    }
                })
                .partitioned(
                    || {
                        RecordAggregator::new(
                            aggregator_config.max_bytes,
                            aggregator_config.max_records,
                        )
                    },
                    aggregator_config.max_wait,
                )
                .partition_limit(
                    || RecordLimiter::new(rps_per_shard, bps_per_shard),
                    Duration::from_secs(1),
                )
                .batched(
                    RecordBatcher::new(batch_config.max_bytes, batch_config.max_bytes),
                    batch_config.max_wait,
                )
                .map(Ok::<_, ()>)
                .forward(kinesis_sink);

            let (worker, _, _) = tokio::join!(fut1, topology_worker, retry_worker);
            worker.unwrap();

            info!("pipeline worker shutdown")
        }));

        (
            Producer::new(sender),
            PipelineHandler {
                worker_handle,
                worker_shutdown: shutdown_tx,
            },
        )
    }
}

fn kinesis_client(region: String, endpoint: Option<String>, local: bool) -> KinesisClient {
    let region = parse_region(region, endpoint);
    let dispatcher =
        rusoto_core::request::HttpClient::new().expect("failed to create request dispatcher");

    if local {
        return KinesisClient::new_with(
            dispatcher,
            StaticProvider::new_minimal("local".to_string(), "development".to_string()),
            region,
        );
    }

    KinesisClient::new_with(dispatcher, CustomChainProvider::new(), region)
}
