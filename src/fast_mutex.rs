//! A simple Mutex implementation
//!
//! This implementation is not meant to be used outside the `LockPool`, as this by itself falls
//! short in many concurrency pitfalls (only ensuring mutual exclusion)
#[cfg(not(loom))]
use core::cell::UnsafeCell;
#[cfg(loom)]
use loom::cell::{UnsafeCell, MutPtr};

use core::ops::{Deref, DerefMut};
use core::fmt::{self, Formatter, Debug, Display};

pub const UNLOCKED: bool = false;
pub const LOCKED: bool = true;

pub struct FastMutex<T> {
    state: atomic!(AtomicBool, ty),
    data: UnsafeCell<T>
}

unsafe impl<T: Send> Send for FastMutex<T> {}
unsafe impl<T: Sync> Sync for FastMutex<T> {}

#[cfg(not(loom))]
pub struct FastMutexGuard<'lock, T> {
    lock: &'lock FastMutex<T>,
    data: &'lock mut T
}

#[cfg(loom)]
pub struct FastMutexGuard<'lock, T> {
    lock: &'lock FastMutex<T>,
    data: MutPtr<T>
}

impl<'lock, T> FastMutexGuard<'lock, T> {
    #[cfg(not(loom))]
    #[inline]
    pub fn new(lock: &'lock FastMutex<T>, data: &'lock mut T) -> Self {
        Self { lock, data }
    }

    #[cfg(loom)]
    pub fn new(lock: &'lock FastMutex<T>, data: MutPtr<T>) -> Self {
        Self { lock, data }
    }
}

impl<'lock, T> Deref for FastMutexGuard<'lock, T> {
    type Target = T;

    #[cfg(not(loom))]
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.data
    }

    #[cfg(loom)]
    fn deref(&self) -> &Self::Target {
        unsafe { self.data.deref() }
    }
}

impl<'lock, T> DerefMut for FastMutexGuard<'lock, T> {
    #[cfg(not(loom))]
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data
    }

    #[cfg(loom)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.data.deref() }
    }
}

impl<'lock, T> Drop for FastMutexGuard<'lock, T> {
    #[inline]
    fn drop(&mut self) {
        unsafe { self.lock.unlock() };
    }
}

impl<T: Default> Default for FastMutex<T> {
    #[inline]
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> FastMutex<T> {
    #[inline]
    pub fn new(data: T) -> Self {
        Self {
            state: atomic!(bool, UNLOCKED),
            data: UnsafeCell::new(data)
        }
    }

    /// # Safety
    ///
    /// If the caller is not the one holding the lock they will cause race conditions.
    #[inline]
    pub unsafe fn unlock(&self) {
        self.state.store(UNLOCKED, ordering!(Release));
    }

    #[cfg(not(loom))]
    #[inline]
    #[allow(clippy::mut_from_ref)]
    unsafe fn get(&self) -> &mut T {
        &mut *self.data.get()
    }

    #[cfg(loom)]
    unsafe fn get(&self) -> MutPtr<T> {
        self.data.get_mut()
    }

    #[inline]
    pub fn quick_try_lock(&self) -> Option<FastMutexGuard<T>> {
        if self.state.compare_exchange(UNLOCKED, LOCKED, ordering!(AcqRel), ordering!(Relaxed)).is_ok() {
            Some(FastMutexGuard::new(self, unsafe { self.get() }))
        } else { None }
    }
}

#[cfg(not(loom))]
impl<T: Display> Display for FastMutexGuard<'_, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.data)
    }
}


#[cfg(not(loom))]
impl<T: Debug> Debug for FastMutexGuard<'_, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("FastMutexGuard")
            .field("data", &self.data)
            .finish()
    }
}

#[cfg(loom)]
impl<T: Display> Display for FastMutexGuard<'_, T> {
    fn fmt(&self, _f: &mut Formatter<'_>) -> fmt::Result {
        Ok(())
    }
}

#[cfg(loom)]
impl<T: Debug> Debug for FastMutexGuard<'_, T> {
    fn fmt(&self, _f: &mut Formatter<'_>) -> fmt::Result {
        Ok(())
    }
}