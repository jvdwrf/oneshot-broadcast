use std::ops::Deref;
use crate::Channel;

#[derive(Debug)]
pub struct Value<T>(Channel<T>);

impl<T> Deref for Value<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

impl<T> Value<T> {
    /// # Safety
    ///
    /// This may only be constructed if the value is initialized
    pub(super) unsafe fn new(channel: Channel<T>) -> Self {
        Self(channel)
    }

    pub fn channel(&self) -> &Channel<T> {
        &self.0
    }

    pub fn get(&self) -> &T {
        debug_assert!(self.0.inner.contains_value());
        // SAFETY: The value is always initialized
        unsafe { self.0.inner.get_unchecked() }
    }
}

impl<T> Clone for Value<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: PartialEq> PartialEq for Value<T> {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl<T: PartialEq> PartialEq<T> for Value<T> {
    fn eq(&self, other: &T) -> bool {
        **self == *other
    }
}

impl<T: Eq> Eq for Value<T> {}

impl<T: PartialOrd> PartialOrd for Value<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        (**self).partial_cmp(&**other)
    }
}

impl<T: PartialOrd> PartialOrd<T> for Value<T> {
    fn partial_cmp(&self, other: &T) -> Option<std::cmp::Ordering> {
        (**self).partial_cmp(other)
    }
}

impl<T: Ord> Ord for Value<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (**self).cmp(&**other)
    }
}

#[allow(clippy::no_effect, unused_must_use)]
fn test(x: Value<String>, y: Value<String>, z: String) {
    x == y;
    x == z;
}
