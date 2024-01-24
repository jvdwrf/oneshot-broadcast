#[doc = include_str!("../README.md")]
pub(crate) mod channel;
pub(crate) mod opt_vec;

use futures::Future;
use owning_ref::OwningRef;
use std::{
    any::Any,
    future::IntoFuture,
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
};

pub use channel::*;

//-------------------------------------
// Channel
//-------------------------------------

#[derive(Debug)]
pub struct Channel<T> {
    inner: Arc<InnerChannel<T>>,
}

impl<T> Clone for Channel<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Channel<T> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(InnerChannel::new()),
        }
    }

    /// Erase the type of the channel.
    pub fn into_any(self) -> AnyChannel
    where
        T: Send + Sync + 'static,
    {
        let inner = OwningRef::new(self.inner).map(|channel| channel.as_any());
        // Safety: We're not moving the address of the channel
        let inner = unsafe {
            inner.map_owner(|channel| {
                let channel: Arc<dyn Any + Send + Sync> = channel;
                channel
            })
        };
        AnyChannel { inner }
    }

    /// Send a value into the channel.
    pub fn send(&self, value: T) -> Result<(), T> {
        self.inner.send(value)
    }

    /// Get the value if it has been sent.
    pub fn get_value(&self) -> Option<Result<&T, Closed>> {
        self.inner.get()
    }

    /// Try to convert the channel into a value.
    pub fn into_value(self) -> Result<Value<T>, Self> {
        if self.inner.contains_value() {
            Ok(unsafe { Value::new(self) })
        } else {
            Err(self)
        }
    }

    /// The number of references to the channel.
    pub fn ref_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }

    /// Closes the channel.
    pub fn close(&mut self) -> bool {
        self.inner.close()
    }

    pub fn recv(&self) -> RecvFut<'_, T> {
        self.into_future()
    }

    pub fn into_recv(self) -> IntoRecvFut<T> {
        self.into_future()
    }
}

impl<T> Default for Channel<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> IntoFuture for Channel<T> {
    type Output = Result<Value<T>, Closed>;
    type IntoFuture = IntoRecvFut<T>;

    fn into_future(self) -> Self::IntoFuture {
        IntoRecvFut {
            channel: Some(self),
            pos: None,
        }
    }
}

impl<'a, T> IntoFuture for &'a Channel<T> {
    type Output = Result<&'a T, Closed>;
    type IntoFuture = RecvFut<'a, T>;

    fn into_future(self) -> Self::IntoFuture {
        self.inner.into_future()
    }
}

impl<'a, T> IntoFuture for &'a mut Channel<T> {
    type Output = Result<&'a T, Closed>;
    type IntoFuture = RecvFut<'a, T>;

    fn into_future(self) -> Self::IntoFuture {
        self.inner.into_future()
    }
}

//-------------------------------------
// IntoRecvFut
//-------------------------------------

pub struct IntoRecvFut<T> {
    channel: Option<Channel<T>>,
    pos: Option<usize>,
}

impl<T> Unpin for IntoRecvFut<T> {}

impl<T> Future for IntoRecvFut<T> {
    type Output = Result<Value<T>, Closed>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        let channel = this.channel.as_mut().unwrap();
        ready!(channel.inner.as_any().poll_with_pos(cx, &mut this.pos))?;

        // SAFETY: The value is initialized here
        let value = unsafe { Value::new(this.channel.take().unwrap()) };
        Poll::Ready(Ok(value))
    }
}

impl<T> Drop for IntoRecvFut<T> {
    fn drop(&mut self) {
        if let Some(channel) = self.channel.take() {
            if let Some(pos) = self.pos {
                channel.inner.as_any().remove(pos);
            }
        }
    }
}

//-------------------------------------
// AnyChannel
//-------------------------------------

pub use value::*;
mod value;

#[derive(Debug)]
pub struct AnyChannel {
    inner: OwningRef<'static, Arc<dyn Any + Send + Sync>, InnerSubChannel>,
}

impl AnyChannel {
    pub fn downcast<T>(&self) -> Option<Channel<T>>
    where
        T: Send + Sync + 'static,
    {
        let owner = OwningRef::as_owner(&self.inner);
        match Arc::downcast::<InnerChannel<T>>(owner.clone()) {
            Ok(channel) => Some(Channel { inner: channel }),
            Err(_) => None,
        }
    }

    /// Get the value if it has been sent.
    pub fn get_value(&self) -> Option<Result<(), Closed>> {
        self.inner.get()
    }

    /// The number of references to the channel.
    pub fn ref_count(&self) -> usize {
        Arc::strong_count(self.inner.as_owner())
    }

    /// Closes the channel.
    pub fn close(&mut self) -> bool {
        self.inner.close()
    }

    pub fn recv(&self) -> AnyRecvFut<'_> {
        self.into_future()
    }

    pub fn into_recv(self) -> IntoAnyRecvFut {
        self.into_future()
    }
}

impl IntoFuture for AnyChannel {
    type Output = Result<(), Closed>;
    type IntoFuture = IntoAnyRecvFut;

    fn into_future(self) -> Self::IntoFuture {
        IntoAnyRecvFut {
            channel: Some(self),
            pos: None,
        }
    }
}

impl<'a> IntoFuture for &'a AnyChannel {
    type Output = Result<(), Closed>;
    type IntoFuture = AnyRecvFut<'a>;

    fn into_future(self) -> Self::IntoFuture {
        self.inner.into_future()
    }
}

impl<'a> IntoFuture for &'a mut AnyChannel {
    type Output = Result<(), Closed>;
    type IntoFuture = AnyRecvFut<'a>;

    fn into_future(self) -> Self::IntoFuture {
        self.inner.into_future()
    }
}

//-------------------------------------
// IntoAnyRecvFut
//-------------------------------------

#[derive(Debug)]
pub struct IntoAnyRecvFut {
    channel: Option<AnyChannel>,
    pos: Option<usize>,
}

impl Unpin for IntoAnyRecvFut {}

impl Future for IntoAnyRecvFut {
    type Output = Result<(), Closed>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        let channel = this.channel.as_mut().unwrap();
        ready!(channel.inner.poll_with_pos(cx, &mut this.pos))?;
        Poll::Ready(Ok(()))
    }
}

impl Drop for IntoAnyRecvFut {
    fn drop(&mut self) {
        if let Some(channel) = self.channel.take() {
            if let Some(pos) = self.pos {
                channel.inner.remove(pos);
            }
        }
    }
}

//-------------------------------------
// Tests
//-------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_send() {
        let channel = Channel::new();

        assert!(channel.send(1).is_ok());
        assert!(channel.send(2).is_err());

        assert_eq!(channel.clone().await.unwrap(), 1);
        channel.clone().await.unwrap();
        assert_eq!(channel.get_value().unwrap().unwrap(), &1);
    }

    #[tokio::test]
    async fn test_listener() {
        let channel = Channel::new();
        let mut any_channel = channel.clone().into_any();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            channel.send(1).ok();
        });

        (&mut any_channel).await.unwrap();
        any_channel.get_value().unwrap().unwrap();
    }

    #[test]
    fn test_downcast() {
        let channel = Channel::<()>::new().into_any();
        assert!(channel.downcast::<()>().is_some());
        assert!(channel.downcast::<i32>().is_none());
    }

    #[tokio::test]
    async fn test_drop_sender() {
        let channel = Channel::<()>::new();
        let channel2 = channel.clone();
        drop(channel);
        channel2.send(()).unwrap();
        assert!(channel2.await.is_ok());
    }

    #[tokio::test]
    async fn test_send_while_receiving() {
        let channel = Channel::new();
        let channel2 = channel.clone();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            channel2.send(()).ok();
        });

        assert!(channel.recv().await.is_ok());
    }

    #[tokio::test]
    async fn test_lifetime() {
        let channel = Channel::new();
        let channel2 = channel.clone();

        channel.send(1).ok();

        {
            let receiver = channel2.clone();
            let fut = receiver.recv();
            let _val = fut.await.unwrap();
            drop(receiver);
            // assert_eq!(_val, &1); // doesn't compile
        }

        {
            let receiver = channel2.clone();
            let mut fut = receiver.recv();
            let _val = (&mut fut).await.unwrap();
            drop(receiver);
            // assert_eq!(_val, &1); // doesn't compile
        }
    }
}
