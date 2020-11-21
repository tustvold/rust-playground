use crate::topology::{ShardId, TopologyGeneration};
use bytes::{Buf, Bytes};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use stream::{Limiter, LimiterError, Partitioned, Reducer, TokenBucket};
use tokio::sync::{mpsc, oneshot};
use tracing::info;

#[derive(Debug, Clone)]
pub enum Error {
    RecordTooLarge,
    WorkerDead,
    AckDropped,
}

#[derive(Debug, Clone)]
pub struct Ack {
    pub shard_id: ShardId,
    pub sequence_number: String,
}

#[derive(Serialize, Deserialize)]
pub struct RawRecord {
    pub partition_key: String,
    pub data: Bytes,
}

#[derive(Debug)]
pub(crate) struct Record {
    pub partition_key: String,
    pub data: Bytes,
    pub predicted_shard_id: Option<(ShardId, TopologyGeneration)>,
    pub acker: Option<oneshot::Sender<Result<Ack, Error>>>,
    pub children: Vec<Record>,
}

impl Record {
    pub fn ack(mut self, result: Result<Ack, Error>) {
        for child in self.children {
            child.ack(result.clone());
        }

        if let Some(ack) = self.acker.take() {
            let _ = ack.send(result);
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn hash_key(&self) -> u128 {
        let mut cursor = std::io::Cursor::new(md5::compute(&self.partition_key).0);
        cursor.get_u128()
    }
}

impl Partitioned for Record {
    type Key = ShardId;

    fn partition(&self) -> Self::Key {
        self.predicted_shard_id.as_ref().unwrap().0
    }
}

pub(crate) struct RecordBatcher {
    buffer: Vec<Record>,
    cur_bytes: usize,
    max_bytes: usize,
    max_records: usize,
}

impl RecordBatcher {
    pub fn new(max_bytes: usize, max_records: usize) -> RecordBatcher {
        RecordBatcher {
            buffer: vec![],
            cur_bytes: 0,
            max_records,
            max_bytes,
        }
    }
}

impl Reducer for RecordBatcher {
    type Item = Record;

    type Output = Vec<Record>;

    fn try_push(&mut self, item: Record) -> Option<Record> {
        let new_bytes = self.cur_bytes.saturating_add(item.len());

        if self.buffer.len() >= self.max_records || new_bytes > self.max_bytes {
            info!("batch full");
            return Some(item);
        }

        self.cur_bytes = new_bytes;
        self.buffer.push(item);
        None
    }

    fn take(&mut self) -> Option<Self::Output> {
        if self.buffer.is_empty() {
            return None;
        }
        info!(
            bytes = self.cur_bytes,
            count = self.buffer.len(),
            "flushing batch"
        );

        self.cur_bytes = 0;
        Some(std::mem::take(self.buffer.as_mut()))
    }

    fn empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

pub(crate) struct RecordLimiter {
    bytes: TokenBucket,
    records: TokenBucket,
}

impl RecordLimiter {
    pub fn new(records_per_second: u64, bytes_per_second: u64) -> RecordLimiter {
        RecordLimiter {
            bytes: TokenBucket::per_second(bytes_per_second),
            records: TokenBucket::per_second(records_per_second),
        }
    }
}

impl Limiter for RecordLimiter {
    type Item = Record;

    fn active(&mut self) -> bool {
        self.records.active() || self.bytes.active()
    }

    fn try_take(&mut self, item: &Self::Item) -> Result<(), LimiterError> {
        self.records.try_take(&1)?;
        self.bytes.try_take(&(item.data.len() as u64))
    }
}

#[derive(Clone)]
pub struct Producer {
    sender: mpsc::Sender<Record>,
}

impl Producer {
    pub(crate) fn new(sender: mpsc::Sender<Record>) -> Producer {
        Producer { sender }
    }

    pub async fn submit(
        &mut self,
        records: impl Iterator<Item = RawRecord>,
    ) -> Vec<Result<Ack, Error>> {
        let stream = FuturesUnordered::new();
        for record in records {
            let (otx, orx) = oneshot::channel::<_>();

            let record = Record {
                partition_key: record.partition_key,
                acker: Some(otx),
                predicted_shard_id: None,
                data: record.data,
                children: vec![],
            };

            let send_result = self.sender.send(record).await;
            stream.push(async move {
                match send_result {
                    Ok(()) => orx.await.map_err(|_| Error::AckDropped)?,
                    Err(_) => Err(Error::WorkerDead),
                }
            });
        }

        stream.collect::<Vec<_>>().await
    }
}
