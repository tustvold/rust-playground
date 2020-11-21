use crate::shutdown;
use futures::future::BoxFuture;
use futures::FutureExt;
use rusoto_core::RusotoError;
use rusoto_kinesis::{Kinesis, KinesisClient, ListShardsError, ListShardsInput};
use std::convert::{TryFrom, TryInto};
use std::str::FromStr;
use tokio::sync::{mpsc, watch};
use tokio::time::{delay_for, Duration};
use tracing::{error, info};

#[derive(Debug)]
enum ControlMessage {
    Flush(TopologyGeneration),
}

#[derive(Debug, Clone)]
pub struct TopologyGeneration(u64);

#[derive(Debug, Clone)]
pub enum Error {
    InvalidShard,
    InvalidShardMap,
    ListShardsError(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl From<RusotoError<ListShardsError>> for Error {
    fn from(e: RusotoError<ListShardsError>) -> Self {
        Error::ListShardsError(e.to_string())
    }
}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct ShardId(u64);

#[derive(Debug, Clone)]
pub struct Shard {
    id: ShardId,
    starting_hash_key: u128,
    ending_hash_key: u128,
}

impl FromStr for ShardId {
    type Err = Error;

    fn from_str(shard: &str) -> Result<Self, Self::Err> {
        let id = shard
            .splitn(2, '-')
            .last()
            .and_then(|x| x.parse().ok())
            .ok_or(Error::InvalidShard)?;
        Ok(ShardId(id))
    }
}

impl ToString for ShardId {
    fn to_string(&self) -> String {
        format!("shardId-{:012}", self.0)
    }
}

impl TryFrom<rusoto_kinesis::Shard> for Shard {
    type Error = Error;

    fn try_from(shard: rusoto_kinesis::Shard) -> Result<Self> {
        let id = shard.shard_id.parse()?;

        let starting_hash_key = shard
            .hash_key_range
            .starting_hash_key
            .parse()
            .map_err(|_| Error::InvalidShard)?;

        let ending_hash_key = shard
            .hash_key_range
            .ending_hash_key
            .parse()
            .map_err(|_| Error::InvalidShard)?;

        Ok(Shard {
            id,
            starting_hash_key,
            ending_hash_key,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Topology {
    open_shards: Vec<Shard>,
}

impl Topology {
    fn new(mut open_shards: Vec<Shard>) -> Result<Topology> {
        open_shards.sort_by_key(|x| x.starting_hash_key);

        let topology = Topology { open_shards };

        if !topology.is_valid() {
            return Err(Error::InvalidShardMap);
        }

        Ok(topology)
    }

    fn is_valid(&self) -> bool {
        if self.open_shards.is_empty() {
            return false;
        }

        let mut hash_key = 0_u128;
        for shard in self.open_shards.iter() {
            if shard.starting_hash_key != hash_key
                || shard.starting_hash_key > shard.ending_hash_key
            {
                return false;
            }
            hash_key = u128::wrapping_add(shard.ending_hash_key, 1)
        }

        hash_key == 0
    }

    pub fn get_shard(&self, hash_key: u128) -> ShardId {
        self.open_shards
            .iter()
            .find_map(|shard| {
                if shard.ending_hash_key >= hash_key {
                    Some(shard.id)
                } else {
                    None
                }
            })
            .unwrap()
    }
}

#[derive(Clone)]
pub struct TopologyService {
    map: watch::Receiver<Option<(Topology, TopologyGeneration)>>,
    control: mpsc::Sender<ControlMessage>,
}

#[derive(Clone)]
struct TopologyClient {
    client: KinesisClient,
    stream_name: String,
}

impl TopologyClient {
    async fn list_shards(&self) -> Result<Topology> {
        let mut next_token = None;
        let mut open_shards: Vec<Shard> = Vec::new();

        loop {
            let input = if next_token.is_some() {
                ListShardsInput {
                    next_token,
                    ..Default::default()
                }
            } else {
                ListShardsInput {
                    stream_name: Some(self.stream_name.clone()),
                    ..Default::default()
                }
            };

            let output = self.client.list_shards(input).await?;

            if let Some(shards) = output.shards {
                for shard in shards {
                    if shard.sequence_number_range.ending_sequence_number.is_none() {
                        open_shards.push(shard.try_into()?);
                    }
                }
            }

            if output.next_token.is_none() {
                break;
            }

            next_token = output.next_token
        }

        Ok(Topology::new(open_shards)?)
    }
}

impl TopologyService {
    pub(crate) fn new(
        client: KinesisClient,
        stream_name: String,
        mut shutdown: shutdown::Receiver,
    ) -> (TopologyService, BoxFuture<'static, ()>) {
        let (tx, rx) = watch::channel(None);

        let (mut control_tx, control_rx) = mpsc::channel(10);
        control_tx
            .try_send(ControlMessage::Flush(TopologyGeneration(0)))
            .unwrap();

        let worker = async move {
            let mut control_rx = control_rx;
            let client = TopologyClient {
                client,
                stream_name,
            };

            let mut generation: u64 = 0;

            loop {
                tokio::select! {
                    _ = &mut shutdown => break,
                    msg = control_rx.recv() => {
                        match msg {
                            Some(ControlMessage::Flush(flush_generation)) => {
                                if flush_generation.0 != generation {
                                    info!("topology generation already flushed");
                                    continue
                                }

                                tx.broadcast(None).unwrap();

                                loop {
                                    info!("refreshing stream topology");
                                    match client.list_shards().await {
                                        Ok(shards) => {
                                            generation += 1;
                                            tx.broadcast(Some((shards, TopologyGeneration(generation)))).unwrap();
                                            info!(generation, "stream topology updated");
                                            break;
                                        }
                                        Err(e) => {
                                            error!("error refreshing stream topology: {:?}", e);
                                            if shutdown.terminating() {
                                                info!("not retrying as terminating");
                                                break
                                            } else {
                                                delay_for(Duration::from_secs(1)).await;
                                            }
                                        }
                                    }
                                }
                            },
                            None => break
                        }
                    }
                }
            }

            info!("topology worker terminated")
        }
            .boxed();

        (
            TopologyService {
                map: rx,
                control: control_tx,
            },
            worker,
        )
    }

    pub async fn lookup_shard(&mut self, hash_key: u128) -> (ShardId, TopologyGeneration) {
        loop {
            if let Some((topology, generation)) = self.map.borrow().as_ref() {
                return (topology.get_shard(hash_key), generation.clone());
            }

            self.map.recv().await.unwrap();
        }
    }

    pub async fn invalidate(&mut self, generation: TopologyGeneration) {
        let _ = self.control.send(ControlMessage::Flush(generation)).await;
    }
}
