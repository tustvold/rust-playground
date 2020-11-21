use futures::{ready, Stream};
use pin_project::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::watch;

#[pin_project]
#[derive(Clone)]
pub(crate) struct Receiver(#[pin] watch::Receiver<bool>);

pub(crate) struct Sender(watch::Sender<bool>);

pub(crate) fn channel() -> (Sender, Receiver) {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    (Sender(shutdown_tx), Receiver(shutdown_rx))
}

impl Receiver {
    pub fn terminating(&self) -> bool {
        *self.0.borrow()
    }
}

impl Future for Receiver {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        while !*self.0.borrow() {
            match ready!(self.as_mut().project().0.poll_next(cx)) {
                Some(true) | None => return Poll::Ready(()),
                _ => continue,
            }
        }
        Poll::Ready(())
    }
}

impl Sender {
    pub fn shutdown(&self) {
        let _ = self.0.broadcast(true);
    }
}
