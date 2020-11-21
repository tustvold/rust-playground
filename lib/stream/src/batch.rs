use std::hash::Hash;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::future::FusedFuture;
use futures::prelude::*;
use futures::ready;
use futures::stream::FuturesUnordered;
use futures::FutureExt;
use pin_project::pin_project;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use tokio::time::{delay_for, Delay, Duration};
use tracing::warn;

pub trait Reducer {
    type Item;

    type Output;

    fn try_push(&mut self, item: Self::Item) -> Option<Self::Item>;

    fn take(&mut self) -> Option<Self::Output>;

    fn empty(&self) -> bool;
}

pub trait Partitioned {
    type Key: Hash + Eq + Clone;

    fn partition(&self) -> Self::Key;
}

pub trait BatchStreamExt: Stream {
    fn batched<R: Reducer<Item = Self::Item>>(
        self,
        reducer: R,
        duration: Duration,
    ) -> Batched<Self, R>
    where
        Self: Sized,
    {
        Batched::new(self, reducer, duration)
    }

    fn partitioned<R: Reducer<Item = Self::Item>, F: Fn() -> R>(
        self,
        reducer_factory: F,
        duration: Duration,
    ) -> PartitionBatched<Self, R, F>
    where
        Self: Sized,
        Self::Item: Partitioned,
    {
        PartitionBatched::new(self, reducer_factory, duration)
    }
}
impl<T: ?Sized> BatchStreamExt for T where T: Stream {}

#[pin_project]
#[must_use = "streams do nothing unless polled"]
pub struct Batched<St: Stream, R: Reducer<Item = St::Item>> {
    #[pin]
    stream: stream::Fuse<St>,
    #[pin]
    clock: future::Fuse<Delay>,

    reducer: R,
    timeout: Duration,
}

impl<St: Stream, R: Reducer<Item = St::Item>> Batched<St, R> {
    fn new(stream: St, reducer: R, timeout: Duration) -> Batched<St, R> {
        Batched {
            stream: stream.fuse(),
            clock: future::Fuse::terminated(),
            reducer,
            timeout,
        }
    }
}

impl<St: Stream, R: Reducer<Item = St::Item>> Stream for Batched<St, R> {
    type Item = R::Output;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match self.as_mut().project().stream.poll_next(cx) {
                Poll::Ready(Some(item)) => {
                    let mut this = self.as_mut().project();
                    let timeout = *this.timeout;

                    if this.clock.is_terminated() {
                        this.clock.set(delay_for(timeout).fuse());
                    }

                    if let Some(item) = this.reducer.try_push(item) {
                        let taken = this.reducer.take();
                        if taken.is_none() || this.reducer.try_push(item).is_some() {
                            warn!("record too large for reducer - dropping");
                            continue;
                        }

                        this.clock.set(delay_for(timeout).fuse());
                        return Poll::Ready(taken);
                    }
                }

                Poll::Ready(None) => {
                    let full_buf = self.as_mut().project().reducer.take();
                    return Poll::Ready(full_buf);
                }

                Poll::Pending => {
                    ready!(self.as_mut().project().clock.poll(cx));
                    return Poll::Ready(self.project().reducer.take());
                }
            }
        }
    }
}

#[pin_project]
struct KeyedDelay<K> {
    #[pin]
    delay: Delay,
    key: Option<K>,
    generation: usize,
}

impl<K> KeyedDelay<K> {
    fn new(delay: Duration, key: K, generation: usize) -> Self {
        Self {
            delay: delay_for(delay),
            key: Some(key),
            generation,
        }
    }
}

impl<K> Future for KeyedDelay<K> {
    type Output = (K, usize);

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        ready!(self.as_mut().project().delay.poll(cx));
        Poll::Ready((self.key.take().unwrap(), self.generation))
    }
}

#[pin_project]
#[must_use = "streams do nothing unless polled"]
pub struct PartitionBatched<St: Stream, R: Reducer<Item = St::Item>, F: Fn() -> R>
where
    St::Item: Partitioned,
{
    #[pin]
    stream: stream::Fuse<St>,
    #[pin]
    timeouts: FuturesUnordered<KeyedDelay<<St::Item as Partitioned>::Key>>,

    reducer_factory: F,
    generation: usize,
    reducers: HashMap<<St::Item as Partitioned>::Key, (R, usize)>,
    timeout: Duration,
}

impl<St: Stream, R: Reducer<Item = St::Item>, F: Fn() -> R> PartitionBatched<St, R, F>
where
    St::Item: Partitioned,
{
    fn new(stream: St, reducer_factory: F, timeout: Duration) -> Self {
        PartitionBatched {
            stream: stream.fuse(),
            timeouts: Default::default(),
            reducers: Default::default(),
            reducer_factory,
            generation: 0,
            timeout,
        }
    }
}

impl<St: Stream, R: Reducer<Item = St::Item>, F: Fn() -> R> Stream for PartitionBatched<St, R, F>
where
    St::Item: Partitioned,
{
    type Item = R::Output;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match self.as_mut().project().stream.poll_next(cx) {
                Poll::Ready(Some(item)) => {
                    let this = self.as_mut().project();
                    let timeout = *this.timeout;

                    let key = item.partition();
                    let (reducer, generation) = match this.reducers.entry(key.clone()) {
                        Entry::Occupied(entry) => entry.into_mut(),
                        Entry::Vacant(entry) => {
                            let ret = entry.insert(((this.reducer_factory)(), *this.generation));
                            this.timeouts.push(KeyedDelay::new(
                                timeout,
                                key.clone(),
                                *this.generation,
                            ));
                            *this.generation += 1;
                            ret
                        }
                    };

                    if let Some(item) = reducer.try_push(item) {
                        let taken = reducer.take();
                        if taken.is_none() || reducer.try_push(item).is_some() {
                            warn!("record too large for reducer - dropping");
                            continue;
                        }

                        *generation += 1;
                        this.timeouts
                            .push(KeyedDelay::new(timeout, key, *generation));
                        return Poll::Ready(taken);
                    }
                }

                Poll::Ready(None) => {
                    while let Some(key) = self.reducers.iter().next().map(|x| x.0.clone()) {
                        let mut reducer = self.as_mut().project().reducers.remove(&key).unwrap().0;
                        if let Some(taken) = reducer.take() {
                            return Poll::Ready(Some(taken));
                        }
                    }
                    return Poll::Ready(None);
                }

                Poll::Pending => {
                    loop {
                        match self.as_mut().project().timeouts.poll_next(cx) {
                            Poll::Ready(Some((key, timeout_generation))) => {
                                match self.reducers.get(&key) {
                                    Some((_, generation)) if *generation == timeout_generation => {
                                        let mut reducer = self
                                            .as_mut()
                                            .project()
                                            .reducers
                                            .remove(&key)
                                            .unwrap()
                                            .0;
                                        if let Some(taken) = reducer.take() {
                                            return Poll::Ready(Some(taken));
                                        }
                                    }
                                    // Stale timeout
                                    _ => {}
                                }
                            }
                            _ => return Poll::Pending,
                        }
                    }
                }
            }
        }
    }
}
