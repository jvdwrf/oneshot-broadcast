pub(crate) mod channel;
pub(crate) mod opt_vec;

use channel::{Channel, DynChannel};
use owning_ref::OwningRef;
use std::{
    any::Any,
    future::Future,
    mem::ManuallyDrop,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

/// Create a new `oneshot-broadcast` channel.
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let channel = Arc::new(Channel::new());
    (
        Sender {
            channel: channel.clone(),
        },
        Receiver { channel, pos: None },
    )
}

/// The sender half of a `oneshot-broadcast` channel.
///
/// This can be used to send a value to all receivers without cloning.
#[derive(Debug)]
pub struct Sender<T> {
    channel: Arc<Channel<T>>,
}

impl<T> Sender<T> {
    /// Send a value to all receivers.
    ///
    /// Does nothing if the value has already been sent.
    pub fn send(&mut self, value: T)
    where
        T: 'static,
    {
        // First we set the value.
        self.channel.set(value);

        // Then we wake all the wakers.
        self.channel.wake_all();
    }

    /// Whether the message has been sent.
    pub fn is_sent(&self) -> bool {
        self.channel.is_sent()
    }

    /// Get the value if it has been sent.
    pub fn get(&self) -> Option<Result<&T, RecvError>> {
        self.channel.get()
    }

    /// The number of receivers, including [`Listener`]s.
    pub fn receiver_count(&self) -> usize {
        Arc::strong_count(&self.channel) - 1
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        self.channel.close()
    }
}

/// The receiver half of a `oneshot-broadcast` channel.
///
/// This can be used to receive a value from the sender, and is cloneable.
/// All receivers will receive a reference to the same value.
///
/// The receiver can be `await`ed to wait for the message, and the
/// value can be retrieved by calling `get`. Alternatively, one can call `recv` to
/// do the same thing.
///
/// Receivers can also be converted into a `Listener` which can be used to wait
/// for the message.
#[derive(Debug)]
pub struct Receiver<T> {
    channel: Arc<Channel<T>>,
    pos: Option<usize>,
}

#[derive(Clone, Copy)]
pub struct RecvError;

impl<T> Receiver<T> {
    /// Wait for the message, and return a reference to the result.
    ///
    /// This is the same as:
    /// ```ignore
    /// let mut receiver: Receiver<_> = ...;
    /// let _ = (&mut receiver).await;
    /// let value = receiver.get().unwrap()
    /// ```
    pub fn recv(&mut self) -> RecvFut<'_, T> {
        RecvFut { receiver: self }
    }

    /// Whether the message has been sent.
    pub fn is_sent(&self) -> bool {
        self.channel.is_sent()
    }

    /// Get the value if it has been sent.
    pub fn get(&self) -> Option<Result<&T, RecvError>> {
        self.channel.get()
    }

    /// The number of receivers, including [`Listener`]s.
    pub fn receiver_count(&self) -> usize {
        Arc::strong_count(&self.channel) - 1
    }

    /// Convert the receiver into a listener that can be used to wait for
    /// the message.
    pub fn into_listener(self) -> Listener
    where
        T: Send + Sync + 'static,
    {
        let (channel, pos) = self.into_parts();
        let dyn_channel = OwningRef::new(channel).map(|channel| channel.inner());
        let dyn_channel = unsafe {
            // Safety: We're not moving the address of the channel
            // T has to be send + Sync for the Listener to be send.
            dyn_channel.map_owner(|channel| {
                let channel: Arc<dyn IsDynChannel> = channel;
                channel
            })
        };
        Listener { dyn_channel, pos }
    }

    fn into_parts(self) -> (Arc<Channel<T>>, Option<usize>) {
        let this = ManuallyDrop::new(self);

        // Safety: We're taking out ALL values of the struct, not causing
        // any leaking, and using manually drop to prevent double drop.
        let channel = unsafe { std::ptr::read(&this.channel) };
        let pos = unsafe { std::ptr::read(&this.pos) };

        (channel, pos)
    }
}

trait IsDynChannel: Send {
    fn get_dyn(&self) -> Option<Result<(), RecvError>>;
}

impl<T: Send + Sync> IsDynChannel for Channel<T> {
    fn get_dyn(&self) -> Option<Result<(), RecvError>> {
        match self.get() {
            Some(Ok(_)) => Some(Ok(())),
            Some(Err(_)) => Some(Err(RecvError)),
            None => None,
        }
    }
}

impl std::fmt::Debug for dyn IsDynChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("dyn IsDynChannel").finish()
    }
}

impl<T> Future for Receiver<T> {
    type Output = Result<(), RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let mut fut = this.channel.inner().listener_fut(&mut this.pos);

        match Pin::new(&mut fut).poll(cx) {
            Poll::Ready(()) => Poll::Ready(match this.get().unwrap() {
                Ok(_) => Ok(()),
                Err(_) => Err(RecvError),
            }),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        Self {
            channel: self.channel.clone(),
            pos: None,
        }
    }
}
impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        if let Some(pos) = self.pos {
            self.channel.inner().remove_pos(pos);
        }
    }
}

/// Future that waits for the message to arrive.
pub struct RecvFut<'a, T> {
    receiver: &'a mut Receiver<T>,
}

impl<'a, T: 'a> Future for RecvFut<'a, T> {
    type Output = Result<&'a T, RecvError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self.receiver).poll(cx) {
            Poll::Ready(_) => {
                // Safety: Lifetime of `val` is tied to the lifetime of `self.receiver`
                // todo: Can this be done without transmute?
                let val: Result<&'_ T, RecvError> = self.receiver.get().unwrap();
                let val: Result<&'a T, RecvError> = unsafe { std::mem::transmute(val) };

                Poll::Ready(val)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<'a, T> Unpin for RecvFut<'a, T> {}

/// A listener that can be used to wait for a message to arrive.
///
/// Can be used to erase the type of the channel.
#[derive(Debug, Clone)]
pub struct Listener {
    dyn_channel: OwningRef<'static, Arc<dyn IsDynChannel>, DynChannel>,
    pos: Option<usize>,
}

impl Listener {
    /// Whether the message has been sent.
    pub fn is_sent(&self) -> bool {
        self.dyn_channel.is_sent()
    }

    /// The number of receivers, including [`Listener`]s.
    pub fn receiver_count(&self) -> usize {
        Arc::strong_count(OwningRef::as_owner(&self.dyn_channel)) - 1
    }

    /// Get the value if it has been sent.
    pub fn get(&self) -> Option<Result<(), RecvError>> {
        self.dyn_channel.as_owner().get_dyn()
    }
}

impl Unpin for Listener {}
impl Future for Listener {
    type Output = Result<(), RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let mut fut = this.dyn_channel.listener_fut(&mut this.pos);

        match Pin::new(&mut fut).poll(cx) {
            Poll::Ready(()) => Poll::Ready(this.get().unwrap()),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        if let Some(pos) = self.pos {
            self.dyn_channel.remove_pos(pos);
        }
    }
}
