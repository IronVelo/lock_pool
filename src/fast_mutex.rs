//! A Fail Fast Mutex
//!
//! This implementation is not meant to be used outside the `LockPool`, as this by itself falls
//! short in many concurrency pitfalls (only ensuring mutual exclusion)
#[cfg(not(loom))]
use core::cell::UnsafeCell;
#[cfg(loom)]
use loom::cell::{UnsafeCell, MutPtr};

use core::ops::{Deref, DerefMut};
use core::fmt::{self, Formatter, Debug, Display};
use core::panic::{RefUnwindSafe, UnwindSafe};

pub const UNLOCKED: bool = false;
pub const LOCKED: bool = true;

/// A lightweight, fail fast, mutex implementation
///
/// <br>
///
/// This was not designed to be used by itself, rather within the `LockPool` as this only ensures
/// the property of mutual exclusion and prevents no other concurrency pitfall. In this crate, the
/// `LockPool` is the synchronization mechanism, and this is merely a component of this.
///
/// # Example
///
/// ```
/// # use lock_pool::fast_mutex::FastMutex;
/// #
/// let mutex = FastMutex::new(7);
///
/// let mut guard = mutex.quick_try_lock().unwrap();
/// *guard *= 2;
/// drop(guard);
///
/// mutex.try_while_locked(|v| assert_eq!(*v, 14)).unwrap();
/// ```
pub struct FastMutex<T> {
    state: atomic!(AtomicBool, ty),
    data: UnsafeCell<T>
}

unsafe impl<T: Send> Send for FastMutex<T> {}
unsafe impl<T: Sync> Sync for FastMutex<T> {}
impl<T> RefUnwindSafe for FastMutex<T> {}
impl<T> UnwindSafe for FastMutex<T> {}

/// The `FastMutex`'s guard, releasing the lock when going out of scope.
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
    /// Create a new Guard for the `FastMutex`
    ///
    /// # Safety
    ///
    /// If the lock is not held by the caller before creation this will cause race conditions.
    #[cfg(not(loom))]
    #[inline]
    pub unsafe fn new(lock: &'lock FastMutex<T>, data: &'lock mut T) -> Self {
        Self { lock, data }
    }

    #[cfg(loom)]
    pub unsafe fn new(lock: &'lock FastMutex<T>, data: MutPtr<T>) -> Self {
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
    /// Release the lock.
    #[inline]
    fn drop(&mut self) {
        // SAFETY: we exist, therefore we are holding the lock.
        unsafe { self.lock.unlock() };
    }
}

impl<T: Default> Default for FastMutex<T> {
    #[inline]
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> From<T> for FastMutex<T> {
    #[inline]
    fn from(value: T) -> Self {
        Self::new(value)
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

    /// Lock the `FastMutex`
    ///
    /// # Returns
    ///
    /// - `true`: The `FastMutex` is now locked and the caller holds said lock.
    /// - `false`: The `FastMutex` was already locked, so the caller does not hold the lock.
    ///
    /// # Safety
    ///
    /// The user must remember to release the lock IFF the lock was successfully acquired.
    /// If the lock was not acquired and the lock is released obviously race conditions will be
    /// caused, if the lock was acquired and the lock not released it will cause a deadlock.
    ///
    /// # Example
    ///
    /// Safe Example
    /// ```
    /// # use lock_pool::fast_mutex::FastMutex;
    /// #
    /// let mutex = FastMutex::new(7);
    ///
    /// // SAFETY: we are respecting the state of the acquisition and releasing the lock after.
    /// if unsafe { mutex.lock() } {
    ///     // ... do something with exclusive access
    ///     // now unlock
    ///
    ///     // SAFETY: we are currently holding the lock.
    ///     unsafe { mutex.unlock() };
    /// }
    /// ```
    ///
    /// Unsafe Example
    /// ```
    /// # use lock_pool::fast_mutex::FastMutex;
    /// #
    /// let mutex = FastMutex::new(7);
    ///
    /// unsafe { mutex.lock() };
    ///
    /// // we have disregarded the output, which puts us in a terrible situation. Here are our
    /// // options.
    /// // - Unlock: we cause race conditions
    /// // - Not unlock: we cause a deadlock
    /// ```
    #[inline]
    #[must_use = "Disregarding the output means your usage causes undefined behavior"]
    pub unsafe fn lock(&self) -> bool {
        self.state.compare_exchange(
            UNLOCKED, LOCKED,
            ordering!(AcqRel), ordering!(Relaxed)
        ).is_ok()
    }

    /// # Safety
    ///
    /// If the caller is not the one holding the lock they will cause race conditions.
    #[inline]
    pub unsafe fn unlock(&self) {
        self.state.store(UNLOCKED, ordering!(Release));
    }

    /// # Safety
    ///
    /// The caller must ensure mutual exclusion
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

    /// Quick! try to acquire the lock.
    ///
    /// # Returns
    ///
    /// - `Some(FastMutexGuard)`: The lock was successfully acquired and the guard is returned.
    /// - `None`: The lock was already held by someone else.
    ///
    /// # Example
    ///
    /// ```
    /// # use lock_pool::fast_mutex::FastMutex;
    /// let mutex = FastMutex::new(7);
    ///
    /// let mut guard = mutex.quick_try_lock().unwrap();
    /// *guard *= 2;
    /// drop(guard);
    ///
    /// unsafe {
    ///     mutex.peek(|v| assert_eq!(*v, 14));
    /// }
    /// ```
    #[inline]
    pub fn quick_try_lock(&self) -> Option<FastMutexGuard<T>> {
        if unsafe { self.lock() }{
            // The guard releases the lock on drop.
            Some(unsafe { FastMutexGuard::new(self, self.get()) })
        } else { None }
    }

    /// Peek the value without synchronization.
    ///
    /// This is primarily used in tests, or moments where you are peeking at some atomic stored
    /// within something which needs synchronization elsewhere.
    ///
    /// # Safety
    ///
    /// This voids all synchronization, there is no assurance of mutual exclusion, any operation
    /// in the peek closure which is not atomic will yield undefined behavior. For quick
    /// modification see [`try_while_locked`]
    ///
    /// # Example
    ///
    /// Safe Example
    /// ```
    /// # use lock_pool::fast_mutex::FastMutex;
    /// #
    /// let mutex = FastMutex::new(7);
    ///
    /// if unsafe { mutex.lock() } {
    ///     // while this usage doesn't make sense, safe usage which generally takes advantage
    ///     // of this api would be nuanced.
    ///
    ///     // SAFETY: we are currently holding the lock.
    ///     unsafe { mutex.peek(|v| assert_eq!(*v, 7)) };
    ///     // SAFETY: we know we're holding the lock
    ///     unsafe { mutex.unlock() };
    /// }
    ///
    /// // this exact example should use `try_while_locked`, not peek, as it is practically
    /// // doing the same thing.
    /// ```
    ///
    /// Unsafe Example
    /// ```
    /// # use lock_pool::fast_mutex::FastMutex;
    /// #
    /// let mutex = FastMutex::new(7);
    ///
    /// // ... pretend this isn't single threaded ...
    /// unsafe { mutex.peek(|v| assert_eq!(*v, 7)) };
    ///
    /// // while this isn't a great example of unsafe usage, acting on the value in any
    /// // serious way here is unsound.
    /// ```
    ///
    /// **Note**:
    ///
    /// If using this, you should test your usage with loom.
    ///
    /// [`try_while_locked`]: FastMutex::try_while_locked
    #[inline]
    pub unsafe fn peek<O, F: Fn(&T) -> O>(&self, peek: F) -> O {
        #[cfg(not(loom))] {
            peek(self.get())
        }
        #[cfg(loom)] {
            self.data.with(|p| peek(unsafe { &*p }))
        }
    }

    /// Try to do something with exclusive access
    ///
    /// # Returns
    ///
    /// - `Some(O)`: The lock was acquired and the closure was invoked.
    /// - `None`:    Failed to acquire the lock.
    ///
    /// # Example
    ///
    /// ```
    /// use lock_pool::fast_mutex::FastMutex;
    ///
    /// let val = FastMutex::new(5);
    /// val.try_while_locked(|v| *v *= 2).unwrap();
    ///
    /// assert!(unsafe { val.peek(|v| *v == 10) })
    /// ```
    #[inline]
    pub fn try_while_locked<O, F: Fn(&mut T) -> O>(&self, f: F) -> Option<O> {
        if unsafe { self.lock() } {
            #[cfg(not(loom))]
            let out = f(unsafe { self.get() });
            #[cfg(loom)]
            let out = self.data.with_mut(|p| f(unsafe {&mut *p}));
            // SAFETY: we are holding the lock.
            unsafe { self.unlock(); }
            Some(out)
        } else { None }
    }

    #[inline]
    #[allow(clippy::match_bool)]
    pub fn display_state(&self) -> &'static str {
        match self.state.load(ordering!(Relaxed)) {
            UNLOCKED => "unlocked",
            LOCKED => "locked"
        }
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

impl<T> Debug for FastMutex<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("FastMutex")
            .field("state", &self.display_state())
            .field("data", &"...")
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