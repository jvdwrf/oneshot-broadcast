// Some code taken from std::sync::OnceLock

use std::{
    cell::UnsafeCell,
    future::Future,
    mem::MaybeUninit,
    pin::Pin,
    sync::{
        atomic::{AtomicU8, Ordering},
        Mutex,
    },
    task::{Context, Poll, Waker},
};

use crate::{opt_vec::OptVec, RecvError};

#[derive(Debug)]
pub(crate) struct Channel<T> {
    value: UnsafeCell<MaybeUninit<T>>,
    inner: InnerChannel,
}

impl<T> Drop for Channel<T> {
    fn drop(&mut self) {
        if self.ready() {
            // SAFETY: The cell was initialized, and this is the only/last ref to it.
            unsafe { (*self.value.get()).assume_init_drop() };
        }
    }
}

unsafe impl<T> Send for Channel<T> where T: Send {}
unsafe impl<T> Sync for Channel<T> where T: Send + Sync {}

impl<T> Channel<T> {
    pub(crate) fn new() -> Self {
        Self {
            value: UnsafeCell::new(MaybeUninit::uninit()),
            inner: InnerChannel {
                wakers: Mutex::new(OptVec::new()),
                state: AtomicU8::new(ACTIVE),
            },
        }
    }

    pub(crate) fn close(&self) -> bool {
        if self.inner.set_state_closed() {
            self.wake_all();
            true
        } else {
            false
        }
    }

    pub(crate) fn get(&self) -> Option<Result<&T, RecvError>> {
        match self.inner.state() {
            // Safety: The value is initialized
            Some(Ok(())) => Some(Ok(unsafe { self.get_unchecked() })),
            Some(Err(_)) => Some(Err(RecvError)),
            None => None,
        }
    }

    pub(crate) fn ready(&self) -> bool {
        self.inner.ready()
    }

    pub(crate) fn inner(&self) -> &InnerChannel {
        &self.inner
    }

    pub(crate) fn wake_all(&self) {
        let mut wakers = self.inner.wakers.lock().unwrap();
        let mut pos = 0;
        while let Some(waker) = wakers.get_mut(pos) {
            if let Some(waker) = waker.take() {
                waker.wake();
            }
            pos += 1;
        }
    }

    pub(crate) fn set(&self, value: T) -> bool {
        // Only runs if the value hasn't been sent yet.
        if self.inner.set_state_sending() {
            // Safe b/c this can only be called once, and no other ref can be accessing this
            unsafe {
                self.value.get().write(MaybeUninit::new(value));
            }
            let res = self.inner.set_state_sent();
            debug_assert!(res);
            true
        } else {
            false
        }
    }

    /// # Safety
    ///
    /// The value must be initialized
    #[inline]
    unsafe fn get_unchecked(&self) -> &T {
        debug_assert!(self.ready());
        (*self.value.get()).assume_init_ref()
    }
}

#[derive(Debug)]
pub(crate) struct InnerChannel {
    state: AtomicU8,
    wakers: Mutex<OptVec<Waker>>,
}

const ACTIVE: u8 = 0;
const SENDING: u8 = 1;
const SENT: u8 = 2;
const CLOSED: u8 = 3;

impl InnerChannel {
    pub(crate) fn ready(&self) -> bool {
        self.state.load(Ordering::Acquire) == SENT
    }

    pub(crate) fn set_state_closed(&self) -> bool {
        self.state
            .compare_exchange(ACTIVE, CLOSED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub(crate) fn set_state_sending(&self) -> bool {
        self.state
            .compare_exchange(ACTIVE, SENDING, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub(crate) fn set_state_sent(&self) -> bool {
        self.state
            .compare_exchange(SENDING, SENT, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub(crate) fn remove_pos(&self, pos: usize) {
        if !self.ready() {
            let mut wakers = self.wakers.lock().unwrap();
            wakers.remove(pos);
        }
    }

    pub(crate) fn exit_fut<'a>(&'a self, pos: &'a mut Option<usize>) -> ExitFut<'a> {
        ExitFut { pos, channel: self }
    }

    /// Uses `Ordering::Acquire` to ensure that the value is initialized.
    pub(crate) fn state(&self) -> Option<Result<(), RecvError>> {
        match self.state.load(Ordering::Acquire) {
            ACTIVE => None,
            SENDING => None,
            SENT => Some(Ok(())),
            CLOSED => Some(Err(RecvError)),
            _ => unreachable!(),
        }
    }
}

pub(crate) struct ExitFut<'a> {
    pos: &'a mut Option<usize>,
    channel: &'a InnerChannel,
}

impl<'a> Unpin for ExitFut<'a> {}
impl<'a> Future for ExitFut<'a> {
    type Output = Result<(), RecvError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Return if the value is ready
        if let Some(state) = self.channel.state() {
            return Poll::Ready(state);
        }

        // Replace our waker if it's not the same as the one we have.
        if let Some(pos) = &self.pos {
            let mut wakers = self.channel.wakers.lock().unwrap();
            let my_waker = wakers.get_mut(*pos).unwrap();

            if !my_waker.as_ref().unwrap().will_wake(cx.waker()) {
                *my_waker = Some(cx.waker().clone());
            }
        }
        // Otherwise, add our waker to the list.
        else {
            let mut wakers = self.channel.wakers.lock().unwrap();
            *self.pos = Some(wakers.add(cx.waker().clone()));
        }

        // Check if the value is ready again.
        if let Some(state) = self.channel.state() {
            Poll::Ready(state)
        } else {
            Poll::Pending
        }
    }
}
