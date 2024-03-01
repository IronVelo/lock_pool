#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(not(loom), no_builtins)]
#![warn(
    clippy::all,
    clippy::nursery,
    clippy::pedantic,
    clippy::cargo,
)]
#![allow(
    clippy::cargo_common_metadata,
    clippy::module_name_repetitions,
    clippy::multiple_crate_versions
)]
#[macro_use]
mod macros;

pub use ref_count::maybe_await;

pub mod fast_mutex;
pub mod future;

use fast_mutex::{FastMutex, FastMutexGuard};
use future::LockFuture;
use ref_count::pad::CacheLine;
use ref_count::{array_queue::ArrayQueue, waiter::Waiter, futures::MaybeFuture};
use core::ops::{Deref, DerefMut};
use core::fmt::{self, Formatter, Debug};
use core::mem::MaybeUninit;

#[cfg(feature = "alloc")]
extern crate alloc;

/// # `LockPool`
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
///
/// # Type Parameters
///
/// - `T`: The type of the objects in the pool.
/// - `SIZE`: The size of the pool, indicating how many resources it manages.
/// - `MAX_WAITERS`: The maximum number of waiters allowed in the pool's queue.
pub struct LockPool<T, const SIZE: usize, const MAX_WAITERS: usize> {
    pool: CacheLine<[FastMutex<T>; SIZE]>,
    #[cfg(feature = "alloc")]
    waiter_queue: alloc::boxed::Box<ArrayQueue<Waiter, MAX_WAITERS>>,
    #[cfg(not(feature = "alloc"))]
    waiter_queue: ArrayQueue<Waiter, MAX_WAITERS>
}

impl<T: Default, const SIZE: usize, const MAX_WAITERS: usize> Default for LockPool<T, SIZE, MAX_WAITERS> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Default, const SIZE: usize, const MAX_WAITERS: usize> LockPool<T, SIZE, MAX_WAITERS> {
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn new() -> Self {
        Self {
            pool: CacheLine::new(core::array::from_fn(|_| FastMutex::default())),
            waiter_queue: alloc::boxed::Box::new(ArrayQueue::new())
        }
    }

    #[cfg(not(feature = "alloc"))]
    #[must_use]
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
    #[must_use]
    pub fn from_fn<F: Fn(usize) -> T>(f: F) -> Self {
        Self {
            pool: CacheLine::new(core::array::from_fn(|i| FastMutex::new(f(i)))),
            waiter_queue: alloc::boxed::Box::new(ArrayQueue::new())
        }
    }

    #[doc = from_fn_doc!()]
    #[cfg(not(feature = "alloc"))]
    #[must_use]
    pub fn from_fn<F: Fn(usize) -> T>(f: F) -> Self {
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
    /// pool. If an object is available, `try_get` returns an Option containing a `LockGuard`, which
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
    ///     Some(_) => panic!("There are no objects remaining."),
    ///     None => 1
    /// };
    /// ```
    ///
    /// [`get`]: LockPool::get
    #[must_use]
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
    #[must_use]
    pub fn get(&self) -> MaybeFuture<LockGuard<T, SIZE, MAX_WAITERS>, LockFuture<T, SIZE, MAX_WAITERS>> {
        self.try_get()
            .map_or_else(|| MaybeFuture::Future(LockFuture::new(self)), MaybeFuture::Ready)
    }

    /// Iterate over the pool, where each element is a `FastMutex`
    pub const fn iter(&self) -> LockPoolIter<T, SIZE, MAX_WAITERS> {
        LockPoolIter {
            idx: 0,
            pool: self
        }
    }
}

/// A guard that provides scoped access to a resource within a `LockPool`.
///
/// This struct is created by calling [`LockPool::try_get`] or [`LockPool::get`] and exists for the
/// lifetime of the resource access. When this guard is dropped, the resource is automatically
/// returned to the pool, and the next waiting task (if any) is notified that the resource is
/// available.
///
/// # Example
///
/// ```
/// use lock_pool::LockPool;
///
/// let pool = LockPool::<usize, 5, 32>::from_fn(|i| i * 10);
/// let guard = pool.try_get().unwrap();
///
/// // Access the protected resource
/// assert_eq!(*guard, 0);
///
/// // `guard` is automatically dropped here, returning the resource to the pool
/// ```
///
/// # Type Parameters
///
/// - `'lock`: The lifetime of the `LockPool` from which this guard was created.
/// - `T`: The type of the protected resource.
/// - `SIZE`: The size of the pool, indicating how many resources it manages.
/// - `MAX_WAITERS`: The maximum number of waiters allowed in the pool's queue.
pub struct LockGuard<'lock, T, const SIZE: usize, const MAX_WAITERS: usize> {
    /// A reference to the `LockPool` that owns the resource.
    pool: &'lock LockPool<T, SIZE, MAX_WAITERS>,
    /// An instance of `FastMutexGuard`, representing the locked state of the resource.
    guard: FastMutexGuard<'lock, T>
}

impl<'lock, T, const SIZE: usize, const MAX_WAITERS: usize> Deref for LockGuard<'lock, T, SIZE, MAX_WAITERS> {
    type Target = T;

    /// Provides immutable access to the protected resource.
    ///
    /// # Returns
    ///
    /// A reference to the protected resource of type `T`.
    #[inline]
    fn deref(&self) -> &Self::Target { &self.guard }
}

impl<'lock, T, const SIZE: usize, const MAX_WAITERS: usize> DerefMut for LockGuard<'lock, T, SIZE, MAX_WAITERS> {
    /// Provides mutable access to the protected resource.
    ///
    /// # Returns
    ///
    /// A mutable reference to the protected resource of type `T`.
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

impl<'lock, T, const SIZE: usize, const MAX_WAITERS: usize> Drop for LockGuard<'lock, T, SIZE, MAX_WAITERS> {
    /// Drops the `LockGuard` and attempts to wake the next waiter in the queue.
    ///
    /// When the guard is dropped, it indicates that the resource is no longer being accessed.
    /// This method checks if there are any tasks waiting for a resource to become available.
    /// If so, it wakes the next task in line, allowing it to acquire the resource.
    ///
    /// # Behavior
    ///
    /// This is an automatic process triggered by the Rust runtime when the `LockGuard` goes out
    /// of scope, ensuring that resources are efficiently managed and that waiting tasks are
    /// promptly serviced.
    ///
    /// # Example
    ///
    /// Dropping a `LockGuard` is typically implicit when it goes out of scope:
    ///
    /// ```rust
    /// use lock_pool::LockPool;
    ///
    /// let pool = LockPool::<usize, 5, 32>::from_fn(|i| i);
    /// {
    ///     let guard = pool.try_get().unwrap();
    ///     // Resource is now locked and accessible via `guard`.
    /// } // `guard` goes out of scope here, and `drop` is called automatically.
    /// ```
    #[inline]
    fn drop(&mut self) {
        if let Some(waiter) = self.pool.waiter_queue.pop() {
            waiter.wake();
        }
    }
}

impl<'lock, T, const SIZE: usize, const MAX_WAITERS: usize> LockGuard<'lock, T, SIZE, MAX_WAITERS> {
    /// Creates a new `LockGuard`.
    ///
    /// This constructor is typically called by the `LockPool` when a resource is successfully
    /// acquired to provide scoped access to that resource.
    ///
    /// # Arguments
    ///
    /// * `pool`  - A reference to the `LockPool` that owns the resource. This ensures that the
    ///            `LockGuard` can notify the pool when the resource is released.
    /// * `guard` - An instance of `FastMutexGuard` representing the locked state of the resource.
    ///             This guard enforces exclusive access to the resource for the duration of its
    ///             lifetime.
    ///
    /// # Returns
    ///
    /// A `LockGuard` instance that provides scoped, exclusive access to a resource within the pool.
    #[inline]
    pub const fn new(pool: &'lock LockPool<T, SIZE, MAX_WAITERS>, guard: FastMutexGuard<'lock, T>) -> Self {
        Self { pool, guard }
    }
}

impl<T: Debug, const SIZE: usize, const MAX_WAITERS: usize> Debug for LockGuard<'_, T, SIZE, MAX_WAITERS> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("LockGuard")
            .field("inner", &self.guard)
            .finish()
    }
}

impl<T, const SIZE: usize, const MAX_WAITERS: usize> From<[T; SIZE]> for LockPool<T, SIZE, MAX_WAITERS> {
    fn from(value: [T; SIZE]) -> Self {
        let mut temp: [MaybeUninit<FastMutex<T>>; SIZE] = unsafe {
            MaybeUninit::uninit().assume_init()
        };
        for i in 0..SIZE {
            // SAFETY: we are only iterating in bounds.
            let slot = unsafe { temp.get_unchecked_mut(i) };
            // SAFETY: we ensure that there is no double free after the loop.
            slot.write(FastMutex::new(unsafe { (value.get_unchecked(i) as *const T).read() }));
        }
        // ensure no double free
        core::mem::forget(value);

        #[cfg(not(feature = "alloc"))] {
        Self {
            // SAFETY: MaybeUninit has the same size, alignment, and ABI as T
            pool: CacheLine::new(unsafe { core::mem::transmute_copy(&temp) }),
            waiter_queue: ArrayQueue::new()
        }}
        #[cfg(feature = "alloc")] {
        Self {
            // SAFETY: MaybeUninit has the same size, alignment, and ABI as T
            pool: CacheLine::new(unsafe { core::mem::transmute_copy(&temp) }),
            waiter_queue: alloc::boxed::Box::new(ArrayQueue::new())
        }}
    }
}

impl<'pool, T, const SIZE: usize, const MAX_WAITERS: usize> IntoIterator for &'pool LockPool<T, SIZE, MAX_WAITERS> {
    type Item = &'pool FastMutex<T>;
    type IntoIter = LockPoolIter<'pool, T, SIZE, MAX_WAITERS>;

    #[inline]
    /// Iterate over the pool, where each element is a `FastMutex`
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[doc(hidden)]
pub struct LockPoolIter<'pool, T, const SIZE: usize, const MAX_WAITERS: usize> {
    pool: &'pool LockPool<T, SIZE, MAX_WAITERS>,
    idx: usize
}

impl<'pool, T, const SIZE: usize, const MAX_WAITERS: usize> Iterator for LockPoolIter<'pool, T, SIZE, MAX_WAITERS> {
    type Item = &'pool FastMutex<T>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.idx < SIZE {
            let old = self.idx;
            self.idx += 1;
            Some( unsafe { self.pool.pool.get_unchecked(old) })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn iter() {
        let pool = LockPool::<usize, 8, 1>::from_fn(|i| i);
        let mut visited = 0usize;

        for data in &pool {
            unsafe { data.peek(|v| assert_eq!(*v, visited)) }
            visited += 1;
        }

        assert_eq!(visited, 8);
    }

    #[test]
    fn from_array() {
        let pool: LockPool<usize, 4, 1> = LockPool::from([0, 1, 2, 3]);

        let g0 = pool.try_get().unwrap();
        assert_eq!(*g0, 0);
        let g1 = pool.try_get().unwrap();
        assert_eq!(*g1, 1);
        let g2 = pool.try_get().unwrap();
        assert_eq!(*g2, 2);
        let g3 = pool.try_get().unwrap();
        assert_eq!(*g3, 3);
        drop(g2);
        let g2 = pool.try_get().unwrap();
        assert_eq!(*g2, 2);
    }

    #[test]
    fn from_array_string() {
        extern crate alloc;
        use alloc::string::String;

        let pool: LockPool<String, 4, 1> = LockPool::from([
            String::from("0"),
            String::from("1"),
            String::from("2"),
            String::from("3")
        ]);

        let g0 = pool.try_get().unwrap();
        assert_eq!(*g0, String::from("0"));
        let g1 = pool.try_get().unwrap();
        assert_eq!(*g1, String::from("1"));
        let g2 = pool.try_get().unwrap();
        assert_eq!(*g2, String::from("2"));
        let g3 = pool.try_get().unwrap();
        assert_eq!(*g3, String::from("3"));
        drop(g2);
        let g2 = pool.try_get().unwrap();
        assert_eq!(*g2, String::from("2"));
    }
}