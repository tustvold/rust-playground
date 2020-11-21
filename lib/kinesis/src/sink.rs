use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::future::{poll_fn, BoxFuture};
use futures::prelude::*;
use futures::stream::FuturesUnordered;
use pin_project::pin_project;
use rusoto_kinesis::{
    Kinesis, KinesisClient, PutRecordsInput, PutRecordsOutput, PutRecordsRequestEntry,
    PutRecordsResultEntry,
};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::DelayQueue;
use tracing::{error, info};

use crate::producer::{Ack, Record};
use crate::shutdown;
use crate::topology::{TopologyGeneration, TopologyService};

#[derive(Clone)]
pub(crate) struct ErrorHandler {
    retry: mpsc::Sender<Record>,
    topology: TopologyService,
}

#[derive(Debug)]
enum Error {
    ThroughputExceeded,
    InternalFailure,
    IncorrectShardPrediction(TopologyGeneration),
    InvalidShard,
}

impl ErrorHandler {
    pub fn new(
        mut retry: mpsc::Sender<Record>,
        topology: TopologyService,
        backoff_delay: Duration,
        mut shutdown: shutdown::Receiver,
    ) -> (ErrorHandler, BoxFuture<'static, ()>) {
        let (tx, mut rx) = mpsc::channel(10);

        let mut delay = DelayQueue::<Record>::new();

        let worker = async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown => break,
                    recv = rx.recv() => match recv {
                        Some(record) => {
                            info!("adding record to backoff queue");
                            delay.insert(record, backoff_delay);
                        },
                        None => break
                    },
                    next = poll_fn(|cx| Pin::new(&mut delay).poll_expired(cx)), if !delay.is_empty() => match next {
                        Some(Ok(record)) => {
                            info!("retrying record");
                            let _ = retry.send(record.into_inner()).await;
                        },
                        Some(Err(e)) => {
                            error!("timeout error - dropping record: {:?}", e);
                        }
                        None => unreachable!("non-empty DelayQueue returned None")
                    }
                }
            }

            info!("retry worker exited")
        };

        (
            ErrorHandler {
                retry: tx,
                topology,
            },
            Box::pin(worker),
        )
    }

    async fn recover(&mut self, record: Record, error: Error) {
        if let Error::IncorrectShardPrediction(generation) = error {
            self.topology.invalidate(generation).await;
        }

        if !record.children.is_empty() {
            for child in record.children {
                let _ = self.retry.send(child).await;
            }
        } else {
            let _ = self.retry.send(record).await;
        }
    }
}

#[pin_project]
pub(crate) struct KinesisSink {
    client: KinesisClient,
    stream_name: String,
    error_handler: ErrorHandler,

    #[pin]
    in_flight: FuturesUnordered<JoinHandle<()>>,
}

impl KinesisSink {
    pub fn new(
        client: KinesisClient,
        stream_name: String,
        error_handler: ErrorHandler,
    ) -> KinesisSink {
        KinesisSink {
            client,
            stream_name,
            error_handler,
            in_flight: Default::default(),
        }
    }
}

fn handle_record(response: PutRecordsResultEntry, record: &Record) -> Result<Ack, Error> {
    match (
        response.sequence_number,
        response.shard_id,
        response.error_code.as_deref(),
    ) {
        (Some(sequence_number), Some(shard_id_str), _) => {
            let shard_id = shard_id_str.parse().map_err(|_| Error::InvalidShard)?;

            if let Some((predicted, generation)) = record.predicted_shard_id.clone() {
                if shard_id != predicted {
                    return Err(Error::IncorrectShardPrediction(generation));
                }
            }

            Ok(Ack {
                shard_id,
                sequence_number,
            })
        }
        (_, _, Some("ProvisionedThroughputExceededException")) => Err(Error::ThroughputExceeded),
        _ => Err(Error::InternalFailure),
    }
}

async fn handle_response(
    response: PutRecordsOutput,
    records: Vec<Record>,
    error_handler: &mut ErrorHandler,
) {
    for (response, record) in response.records.into_iter().zip(records.into_iter()) {
        match handle_record(response, &record) {
            Ok(ack) => {
                record.ack(Ok(ack));
            }
            Err(e) => {
                error!("record error: {:?}", e);
                error_handler.recover(record, e).await;
            }
        }
    }
}

impl Sink<Vec<Record>> for KinesisSink {
    type Error = ();

    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: Vec<Record>) -> Result<(), Self::Error> {
        if item.is_empty() {
            return Ok(());
        }

        info!(count = item.len(), "submitting records");

        let records = item
            .iter()
            .map(|record| PutRecordsRequestEntry {
                data: record.data.clone(),
                explicit_hash_key: None,
                partition_key: record.partition_key.clone(),
            })
            .collect();

        let input = PutRecordsInput {
            records,
            stream_name: self.stream_name.clone(),
        };

        let mut error_handler = self.error_handler.clone();
        let client = self.client.clone();

        let task = tokio::spawn(async move {
            match client.put_records(input).await {
                Ok(response) => handle_response(response, item, &mut error_handler).await,
                Err(e) => {
                    error!("error putting records: {:?}", e);
                    for record in item {
                        error_handler.recover(record, Error::InternalFailure).await;
                    }
                }
            }
        });

        self.in_flight.push(task);

        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let mut this = self.project();
        loop {
            match this.in_flight.as_mut().poll_next(cx) {
                Poll::Ready(Some(_)) => {}
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Pending => return Poll::Pending,
            }
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.poll_flush(cx)
    }
}
