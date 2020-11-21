use std::cmp::min;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use futures::future::{Fuse, FusedFuture};
use futures::{ready, Future, FutureExt};
use pin_project::pin_project;
use tokio::stream::Stream;
use tokio::time::{delay_for, Delay};
use tracing::{error, info};

use crate::batch::Partitioned;

const NANOS_PER_SEC: u64 = 1_000_000_000;

pub trait Limiter {
    type Item;

    fn active(&mut self) -> bool;

    fn try_take(&mut self, item: &Self::Item) -> Result<()>;
}

pub enum Error {
    LimitExceeded(Duration),
    CapacityExceeded,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub struct TokenBucket {
    level: u64,
    token_interval: u64,
    last_time: Instant,
}

impl TokenBucket {
    pub fn per_second(capacity: u64) -> TokenBucket {
        let token_interval = NANOS_PER_SEC / capacity;

        TokenBucket {
            token_interval,
            level: 0,
            last_time: Instant::now().checked_sub(Duration::from_secs(1)).unwrap(),
        }
    }
}

impl Limiter for TokenBucket {
    type Item = u64;

    fn active(&mut self) -> bool {
        Instant::now().duration_since(self.last_time).as_secs() == 0
    }

    fn try_take(&mut self, n: &u64) -> Result<()> {
        let delta = self.token_interval * n;
        if delta > NANOS_PER_SEC {
            return Err(Error::CapacityExceeded);
        }

        let now = Instant::now();
        let new_level = delta + self.level
            - min(
                now.duration_since(self.last_time).as_nanos() as u64,
                self.level,
            );

        if new_level > NANOS_PER_SEC {
            return Err(Error::LimitExceeded(Duration::from_nanos(
                new_level - NANOS_PER_SEC,
            )));
        }
        self.level = new_level;
        self.last_time = now;

        Ok(())
    }
}

pub struct PartitionedLimiter<L: Limiter + Sized, F: Fn() -> L>
where
    L::Item: Partitioned,
{
    inner: HashMap<<L::Item as Partitioned>::Key, L>,
    last_prune: Instant,
    prune_interval: Duration,
    limiter_factory: F,
}

impl<L: Limiter + Sized, F: Fn() -> L> PartitionedLimiter<L, F>
where
    L::Item: Partitioned,
{
    fn new(limiter_factory: F, prune_interval: Duration) -> Self {
        Self {
            inner: Default::default(),
            last_prune: Instant::now(),
            prune_interval,
            limiter_factory,
        }
    }

    fn prune(&mut self) {
        let now = Instant::now();

        if now.duration_since(self.last_prune) > self.prune_interval {
            self.inner.retain(|_, limiter| limiter.active());
            self.last_prune = now;
        }
    }
}

impl<L: Limiter + Sized, F: Fn() -> L> Limiter for PartitionedLimiter<L, F>
where
    L::Item: Partitioned,
{
    type Item = L::Item;

    fn active(&mut self) -> bool {
        self.prune();
        !self.inner.is_empty()
    }

    fn try_take(&mut self, item: &Self::Item) -> Result<(), Error> {
        self.prune();
        match self.inner.entry(item.partition()) {
            Entry::Occupied(entry) => entry.into_mut().try_take(item),
            Entry::Vacant(entry) => entry.insert((self.limiter_factory)()).try_take(item),
        }
    }
}

#[pin_project]
pub struct LimitedStream<S: Stream, L: Limiter<Item = S::Item>> {
    #[pin]
    inner: S,
    #[pin]
    delay: Fuse<Delay>,
    limiter: L,
    buffer: Option<S::Item>,
}

impl<S: Stream, L: Limiter<Item = S::Item>> LimitedStream<S, L> {
    fn new(inner: S, limiter: L) -> Self {
        LimitedStream {
            limiter,
            inner,
            delay: Fuse::terminated(),
            buffer: None,
        }
    }
}

impl<S: Stream, L: Limiter<Item = S::Item>> Stream for LimitedStream<S, L> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            if !self.delay.is_terminated() {
                ready!(self.as_mut().project().delay.poll(cx));
            }

            let mut this = self.as_mut().project();

            if let Some(buffered) = this.buffer.as_ref() {
                match this.limiter.try_take(buffered) {
                    Ok(()) => return Poll::Ready(this.buffer.take()),
                    Err(Error::LimitExceeded(delay)) => {
                        info!(?delay, "limit exceeded");
                        this.delay.set(delay_for(delay).fuse());
                        ready!(this.delay.poll(cx));
                    }
                    Err(Error::CapacityExceeded) => {
                        error!("item exceeded limiter capacity - dropping")
                    }
                }
            }

            if let Some(x) = ready!(this.inner.poll_next(cx)) {
                *self.as_mut().project().buffer = Some(x)
            } else {
                return Poll::Ready(None);
            }
        }
    }
}

pub trait LimitedStreamExt: Stream {
    fn limit<L: Limiter<Item = Self::Item>>(self, limiter: L) -> LimitedStream<Self, L>
    where
        Self: Sized,
    {
        LimitedStream::new(self, limiter)
    }

    fn partition_limit<L: Limiter<Item = Self::Item> + Sized, F: Fn() -> L>(
        self,
        limiter_factory: F,
        prune_interval: Duration,
    ) -> LimitedStream<Self, PartitionedLimiter<L, F>>
    where
        Self: Sized,
        Self::Item: Partitioned,
    {
        LimitedStream::new(
            self,
            PartitionedLimiter::new(limiter_factory, prune_interval),
        )
    }
}
impl<T: ?Sized> LimitedStreamExt for T where T: Stream {}
