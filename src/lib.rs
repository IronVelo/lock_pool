#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(not(loom), no_builtins)]

#[macro_use]
mod macros;

pub use ref_count::maybe_await;

mod fast_mutex;
pub mod future;

use fast_mutex::{FastMutex, FastMutexGuard};
use future::LockFuture;
use ref_count::pad::CacheLine;
use ref_count::{array_queue::ArrayQueue, waiter::Waiter, futures::MaybeFuture};
use core::ops::{Deref, DerefMut};
use core::fmt::{self, Formatter, Debug, Display};

#[cfg(feature = "alloc")]
extern crate alloc;

/// # LockPool
///
/// A high performance, fair, reliable, asynchronous object pool implementation.
///
/// ## Basic Usage
///
/// ```
/// use lock_pool::LockPool;
/// let pool = LockPool::<usize, 5, 32>::from_fn(|i| i);
///
/// let g0 = pool.try_get().unwrap();
/// assert_eq!(*g0, 0);
/// let g1 = pool.try_get().unwrap();
/// assert_eq!(*g1, 1);
/// let g2 = pool.try_get().unwrap();
/// assert_eq!(*g2, 2);
/// let g3 = pool.try_get().unwrap();
/// assert_eq!(*g3, 3);
/// let g4 = pool.try_get().unwrap();
/// assert_eq!(*g4, 4);
///
/// assert!(pool.try_get().is_none());
///
/// drop(g0);
/// assert_eq!(*pool.try_get().unwrap(), 0);
/// ```
pub struct LockPool<T, const SIZE: usize, const MAX_WAITERS: usize> {
    pool: CacheLine<[FastMutex<T>; SIZE]>,
    #[cfg(feature = "alloc")]
    waiter_queue: alloc::boxed::Box<ArrayQueue<Waiter, MAX_WAITERS>>,
    #[cfg(not(feature = "alloc"))]
    waiter_queue: ArrayQueue<Waiter, MAX_WAITERS>
}

impl<T: Default, const SIZE: usize, const MAX_WAITERS: usize> LockPool<T, SIZE, MAX_WAITERS> {
    #[cfg(feature = "alloc")]
    pub fn new() -> Self {
        Self {
            pool: CacheLine::new(core::array::from_fn(|_| FastMutex::default())),
            waiter_queue: alloc::boxed::Box::new(ArrayQueue::new())
        }
    }

    #[cfg(not(feature = "alloc"))]
    pub fn new() -> Self {
        Self {
            pool: CacheLine::new(core::array::from_fn(|_| FastMutex::default())),
            waiter_queue: ArrayQueue::new()
        }
    }
}

macro_rules! from_fn_doc {
    () => {
        r#"
        # From Function

        Create a new `LockPool` from a function, similar to [`core::array::from_fn`].

        ### Closure Arguments

        * `idx` - The index of the object in the pool.

        # Example

        ```
        use lock_pool::LockPool;

        // create lock pool with 5 elements, where the elements are just the indexes.
        let lock_pool = LockPool::<usize, 5, 32>::from_fn(|idx| idx);
        let first_elem = lock_pool.try_get().unwrap();

        assert_eq!(*first_elem, 0);
        ```
        "#
    };
}

impl<T, const SIZE: usize, const MAX_WAITERS: usize> LockPool<T, SIZE, MAX_WAITERS> {
    #[doc = from_fn_doc!()]
    #[cfg(feature = "alloc")]
    pub fn from_fn<F: Fn(usize) -> T>(f: F) -> LockPool<T, SIZE, MAX_WAITERS> {
        Self {
            pool: CacheLine::new(core::array::from_fn(|i| FastMutex::new(f(i)))),
            waiter_queue: alloc::boxed::Box::new(ArrayQueue::new())
        }
    }

    #[doc = from_fn_doc!()]
    #[cfg(not(feature = "alloc"))]
    pub fn from_fn<F: Fn(usize) -> T>(f: F) -> LockPool<T, SIZE, MAX_WAITERS> {
        Self {
            pool: CacheLine::new(core::array::from_fn(|i| FastMutex::new(f(i)))),
            waiter_queue: ArrayQueue::new()
        }
    }
}

impl<T, const SIZE: usize, const MAX_WAITERS: usize> LockPool<T, SIZE, MAX_WAITERS> {
    /// # Try Get
    ///
    /// This method performs a non-blocking attempt to obtain a lock on one of the objects in the
    /// pool. If an object is available, try_get returns an Option containing a LockGuard, which
    /// provides exclusive access to the object. If no objects are available, it returns None. If
    /// you need to wait for an object see [`get`].
    ///
    /// # Use Case
    ///
    /// In the context of connection pooling, you may want to create a new connection rather than
    /// waiting for one to become available. This is the best option for said use case.
    ///
    /// # Example
    ///
    /// ```
    /// use lock_pool::LockPool;
    /// // create a pool of one object.
    /// let pool = LockPool::<usize, 1, 32>::from_fn(|i| i);
    ///
    /// let guard = pool.try_get().unwrap();
    /// assert_eq!(*guard, 0);
    ///
    /// let _new = match pool.try_get() {
    ///     Some(_) => panic!("There's no objects remaining."),
    ///     None => 1
    /// };
    /// ```
    ///
    /// [`get`]: LockPool::get
    #[inline]
    pub fn try_get(&self) -> Option<LockGuard<T, SIZE, MAX_WAITERS>> {
        for i in 0..SIZE {
            // Safety: we are only iterating in bounds
            if let Some(guard) = unsafe { self.pool.get_unchecked(i) }.quick_try_lock() {
                return Some(LockGuard::new(self, guard));
            }
        }
        None
    }

    /// # Get
    ///
    /// Asynchronously acquires a lock on an object within the pool. If an object is immediately
    /// available, this method returns a `MaybeFuture::Ready` containing a `LockGuard`. If no
    /// objects are available, it returns a `MaybeFuture::Future`, which resolves to a `LockGuard`
    /// when an object becomes available. This method is ideal for scenarios where waiting for an
    /// object is acceptable and desired to avoid creating excessive resources (e.g., database
    /// connections). [`MaybeFuture`] is from the [`ref_count`] crate, it is most ergonomic to use
    /// the `maybe_await` macro, re-exported from this crate.
    ///
    /// # Example
    ///
    /// ```rust
    /// use lock_pool::{LockPool, maybe_await};
    /// # use futures::executor;
    ///
    /// async fn example() {
    ///     // create a pool of two objects.
    ///     let pool = LockPool::<usize, 2, 32>::from_fn(|i| i);
    ///     let guard = maybe_await!(pool.get());
    ///
    ///     assert_eq!(*guard, 0);
    /// }
    /// # executor::block_on(example());
    /// ```
    ///
    /// [`MaybeFuture`]: ref_count::futures::MaybeFuture
    pub fn get(&self) -> MaybeFuture<LockGuard<T, SIZE, MAX_WAITERS>, LockFuture<T, SIZE, MAX_WAITERS>> {
        if let Some(guard) = self.try_get() {
            MaybeFuture::Ready(guard)
        } else {
            MaybeFuture::Future(LockFuture::new(self))
        }
    }
}

pub struct LockGuard<'lock, T, const SIZE: usize, const MAX_WAITERS: usize> {
    pool: &'lock LockPool<T, SIZE, MAX_WAITERS>,
    guard: FastMutexGuard<'lock, T>
}

impl<'lock, T, const SIZE: usize, const MAX_WAITERS: usize> Deref for LockGuard<'lock, T, SIZE, MAX_WAITERS> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target { &self.guard }
}

impl<'lock, T, const SIZE: usize, const MAX_WAITERS: usize> DerefMut for LockGuard<'lock, T, SIZE, MAX_WAITERS> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

impl<'lock, T, const SIZE: usize, const MAX_WAITERS: usize> Drop for LockGuard<'lock, T, SIZE, MAX_WAITERS> {
    #[inline]
    fn drop(&mut self) {
        if let Some(waiter) = self.pool.waiter_queue.pop() {
            waiter.wake()
        }
    }
}

impl<'lock, T, const SIZE: usize, const MAX_WAITERS: usize> LockGuard<'lock, T, SIZE, MAX_WAITERS> {
    #[inline]
    pub const fn new(pool: &'lock LockPool<T, SIZE, MAX_WAITERS>, guard: FastMutexGuard<'lock, T>) -> Self {
        Self { pool, guard }
    }
}

impl<T: Display, const SIZE: usize, const MAX_WAITERS: usize> Display for LockGuard<'_, T, SIZE, MAX_WAITERS> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.guard)
    }
}

impl<T: Debug, const SIZE: usize, const MAX_WAITERS: usize> Debug for LockGuard<'_, T, SIZE, MAX_WAITERS> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("LockGuard")
            .field("inner", &self.guard)
            .finish()
    }
}