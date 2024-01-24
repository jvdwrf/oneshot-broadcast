// Some code taken from std::sync::OnceLock

use crate::opt_vec::OptVec;
use std::{
    cell::UnsafeCell,
    future::{Future, IntoFuture},
    mem::MaybeUninit,
    pin::Pin,
    sync::{
        atomic::{AtomicU8, Ordering},
        Mutex,
    },
    task::{ready, Context, Poll, Waker},
};

/// The channel was closed before the message was sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Closed;

#[derive(Debug)]
pub struct InnerChannel<T> {
    value: UnsafeCell<MaybeUninit<T>>,
    any: InnerSubChannel,
}

impl<T> Unpin for InnerChannel<T> {}

impl<T> Drop for InnerChannel<T> {
    fn drop(&mut self) {
        if self.contains_value() {
            // SAFETY: The cell was initialized, and this is the only/last ref to it.
            unsafe { (*self.value.get()).assume_init_drop() };
        }
    }
}

unsafe impl<T> Send for InnerChannel<T> where T: Send {}
unsafe impl<T> Sync for InnerChannel<T> where T: Send + Sync {}

impl<T> InnerChannel<T> {
    pub fn poll_with_pos(
        &self,
        cx: &mut Context<'_>,
        pos: &mut Option<usize>,
    ) -> Poll<Result<&T, Closed>> {
        ready!(self.as_any().poll_with_pos(cx, pos)?);
        Poll::Ready(self.get().unwrap())
    }

    pub fn as_any(&self) -> &InnerSubChannel {
        &self.any
    }

    /// Creates a new channel.
    pub fn new() -> Self {
        Self {
            value: UnsafeCell::new(MaybeUninit::uninit()),
            any: InnerSubChannel {
                wakers: Mutex::new(OptVec::new()),
                state: AtomicU8::new(OPEN),
            },
        }
    }

    /// Closes the channel.
    ///
    /// Returns true if the channel was open, false if it was already closed or a message
    /// was already sent.
    pub fn close(&self) -> bool {
        self.as_any().close()
    }

    /// Returns true if the channel contains a value `T`
    pub fn contains_value(&self) -> bool {
        self.as_any().initialized()
    }

    /// Send a value to the channel, failing if the value has already been sent.
    pub fn send(&self, value: T) -> Result<(), T> {
        // First we set the value.
        self.set(value)?;

        // Then we wake all the wakers.
        self.wake_all();

        Ok(())
    }

    /// Wake all the listeners.
    fn wake_all(&self) {
        self.as_any().wake_all();
    }

    /// Sets a value, without waking the listeners.
    fn set(&self, value: T) -> Result<(), T> {
        // Only runs if the value hasn't been sent yet.
        if self
            .any
            .state
            .compare_exchange(OPEN, SENDING, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            // Safe b/c this can only be called once, and no other ref can be accessing this
            unsafe {
                self.value.get().write(MaybeUninit::new(value));
            }

            debug_assert!(self
                .any
                .state
                .compare_exchange(SENDING, INITIALIZED, Ordering::AcqRel, Ordering::Acquire)
                .is_ok());
            Ok(())
        } else {
            Err(value)
        }
    }

    /// Takes out the value if it's initialized.
    ///
    /// This sets the state to `OPEN` if successful.
    pub fn take(&mut self) -> Option<T> {
        if self.contains_value() {
            // SAFETY: The cell was initialized, and we have mutable access
            let val: T = unsafe { (*self.value.get()).assume_init_read() };
            self.any.state.store(OPEN, Ordering::Release);
            Some(val)
        } else {
            None
        }
    }

    /// Get the value if it's ready.
    pub fn get(&self) -> Option<Result<&T, Closed>> {
        match self.any.get() {
            // Safety: The value is initialized
            Some(Ok(())) => Some(Ok(unsafe { self.get_unchecked() })),
            Some(Err(_)) => Some(Err(Closed)),
            None => None,
        }
    }

    /// # Safety
    ///
    /// The value must be initialized
    #[inline]
    pub unsafe fn get_unchecked(&self) -> &T {
        debug_assert!(self.contains_value());
        (*self.value.get()).assume_init_ref()
    }
}

impl<'a, T> IntoFuture for &'a InnerChannel<T> {
    type Output = Result<&'a T, Closed>;
    type IntoFuture = RecvFut<'a, T>;

    fn into_future(self) -> Self::IntoFuture {
        RecvFut {
            channel: self,
            pos: None,
        }
    }
}

pub struct RecvFut<'a, T> {
    channel: &'a InnerChannel<T>,
    pos: Option<usize>,
}

impl<'a, T> Unpin for RecvFut<'a, T> {}

impl<'a, T> Future for RecvFut<'a, T> {
    type Output = Result<&'a T, Closed>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.channel.poll_with_pos(cx, &mut self.pos)
    }
}

impl<T> Default for InnerChannel<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct InnerSubChannel {
    state: AtomicU8,
    wakers: Mutex<OptVec<Waker>>,
}

const OPEN: u8 = 0;
const SENDING: u8 = 1;
const INITIALIZED: u8 = 2;
const CLOSED: u8 = 3;

impl InnerSubChannel {
    pub fn poll_with_pos(
        &self,
        cx: &mut Context<'_>,
        pos: &mut Option<usize>,
    ) -> Poll<Result<(), Closed>> {
        // Return if the value is ready
        if let Some(state) = self.get() {
            return Poll::Ready(state);
        }

        // Replace our waker if it's not the same as the one we have.
        if let Some(pos) = pos {
            let mut wakers = self.wakers.lock().unwrap();
            let my_waker = wakers.get_mut(*pos).unwrap();

            if !my_waker.as_ref().unwrap().will_wake(cx.waker()) {
                *my_waker = Some(cx.waker().clone());
            }
        }
        // Otherwise, add our waker to the list.
        else {
            let mut wakers = self.wakers.lock().unwrap();
            *pos = Some(wakers.add(cx.waker().clone()));
        }

        // Check if the value is ready again.
        if let Some(state) = self.get() {
            Poll::Ready(state)
        } else {
            Poll::Pending
        }
    }

    pub fn close(&self) -> bool {
        if self
            .state
            .compare_exchange(OPEN, CLOSED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.wake_all();
            true
        } else {
            false
        }
    }

    pub fn wake_all(&self) {
        let mut wakers = self.wakers.lock().unwrap();
        let mut pos = 0;
        while let Some(waker) = wakers.get_mut(pos) {
            if let Some(waker) = waker.take() {
                waker.wake();
            }
            pos += 1;
        }
    }

    pub(crate) fn initialized(&self) -> bool {
        self.state.load(Ordering::Acquire) == INITIALIZED
    }

    pub(crate) fn remove(&self, pos: usize) {
        if !self.initialized() {
            let mut wakers = self.wakers.lock().unwrap();
            wakers.remove(pos);
        }
    }

    /// Uses `Ordering::Acquire` to ensure that the value is initialized.
    pub(crate) fn get(&self) -> Option<Result<(), Closed>> {
        match self.state.load(Ordering::Acquire) {
            OPEN => None,
            SENDING => None,
            INITIALIZED => Some(Ok(())),
            CLOSED => Some(Err(Closed)),
            _ => unreachable!(),
        }
    }
}

impl<'a> IntoFuture for &'a InnerSubChannel {
    type Output = Result<(), Closed>;
    type IntoFuture = AnyRecvFut<'a>;

    fn into_future(self) -> Self::IntoFuture {
        AnyRecvFut {
            pos: None,
            channel: self,
        }
    }
}

#[derive(Debug)]
pub struct AnyRecvFut<'a> {
    pos: Option<usize>,
    channel: &'a InnerSubChannel,
}

impl Clone for AnyRecvFut<'_> {
    fn clone(&self) -> Self {
        Self {
            pos: None,
            channel: self.channel,
        }
    }
}

impl<'a> Unpin for AnyRecvFut<'a> {}

impl<'a> Future for AnyRecvFut<'a> {
    type Output = Result<(), Closed>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        this.channel.poll_with_pos(cx, &mut this.pos)
    }
}

impl<'a> Drop for AnyRecvFut<'a> {
    fn drop(&mut self) {
        if let Some(pos) = self.pos {
            self.channel.remove(pos);
        }
    }
}
