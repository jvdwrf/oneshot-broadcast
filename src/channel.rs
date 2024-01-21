use std::{
    cell::UnsafeCell,
    future::Future,
    mem::MaybeUninit,
    pin::Pin,
    sync::{Mutex, Once, atomic::AtomicU8},
    task::{Context, Poll, Waker},
};

use crate::{opt_vec::OptVec, RecvError};

#[derive(Debug)]
pub(crate) struct Channel<T> {
    value: UnsafeCell<MaybeUninit<Result<T, RecvError>>>,
    dyn_channel: DynChannel,
}

unsafe impl<T> Send for Channel<T> where T: Send + Sync {}

impl<T> Channel<T> {
    pub(crate) fn new() -> Self {
        Self {
            value: UnsafeCell::new(MaybeUninit::uninit()),
            dyn_channel: DynChannel {
                once: Once::new(),
                wakers: Mutex::new(OptVec::new()),
            },
        }
    }

    pub(crate) fn close(&self) {
        // Only runs if the value hasn't been sent yet.
        self.dyn_channel.once.call_once(|| {
            // Safe b/c we're the only thread that can access the value
            unsafe {
                self.value
                    .get()
                    .write(MaybeUninit::new(Err(RecvError)));
            }
        });
    }

    pub(crate) fn get(&self) -> Option<Result<&T, RecvError>> {
        if self.is_sent() {
            // Safe b/c checked for initialization
            let val = unsafe { self.get_unchecked() };
            match val {
                Ok(val) => Some(Ok(val)),
                Err(err) => Some(Err(*err)),
            }
        } else {
            None
        }
    }

    pub(crate) fn is_sent(&self) -> bool {
        self.dyn_channel.once.is_completed()
    }

    pub(crate) fn inner(&self) -> &DynChannel {
        &self.dyn_channel
    }

    pub(crate) fn wake_all(&self) {
        let mut wakers = self.dyn_channel.wakers.lock().unwrap();
        let mut pos = 0;
        while let Some(waker) = wakers.get_mut(pos) {
            if let Some(waker) = waker.take() {
                waker.wake();
            }
            pos += 1;
        }
    }

    pub(crate) fn set(&self, value: T) {
        self.dyn_channel.once.call_once(|| {
            // Safe b/c we're the only thread that can access the value
            unsafe {
                self.value.get().write(MaybeUninit::new(Ok(value)));
            }
        });
    }

    /// # Safety
    ///
    /// The value must be initialized
    #[inline]
    unsafe fn get_unchecked(&self) -> &Result<T, RecvError> {
        debug_assert!(self.dyn_channel.once.is_completed());
        (*self.value.get()).assume_init_ref()
    }
}

#[derive(Debug)]
pub(crate) struct DynChannel {
    once: Once,
    wakers: Mutex<OptVec<Waker>>,
}

impl DynChannel {
    pub(crate) fn remove_pos(&self, pos: usize) {
        if !self.once.is_completed() {
            let mut wakers = self.wakers.lock().unwrap();
            wakers.remove(pos);
        }
    }

    pub(crate) fn listener_fut<'a>(&'a self, pos: &'a mut Option<usize>) -> ExitFut<'a> {
        ExitFut {
            pos,
            once: &self.once,
            wakers: &self.wakers,
        }
    }

    pub(crate) fn is_sent(&self) -> bool {
        self.once.is_completed()
    }
}

pub(crate) struct ExitFut<'a> {
    pos: &'a mut Option<usize>,
    once: &'a Once,
    wakers: &'a Mutex<OptVec<Waker>>,
}

impl<'a> Unpin for ExitFut<'a> {}
impl<'a> Future for ExitFut<'a> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Return if the value is ready
        if self.once.is_completed() {
            return Poll::Ready(());
        }

        // Replace our waker if it's not the same as the one we have.
        if let Some(pos) = &self.pos {
            let mut wakers = self.wakers.lock().unwrap();

            let my_waker = wakers.get_mut(*pos).unwrap();
            if !my_waker.as_ref().unwrap().will_wake(cx.waker()) {
                *my_waker = Some(cx.waker().clone());
            }
        }
        // Otherwise, add our waker to the list.
        else {
            let mut wakers = self.wakers.lock().unwrap();

            *self.pos = Some(wakers.add(cx.waker().clone()));
        }

        // Check if the value is ready again.
        if self.once.is_completed() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}
