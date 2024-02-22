//! Future for `LockGuard`

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use ref_count::waiter::Waiter;
use crate::{LockGuard, LockPool};

pub struct LockFuture<'lock, T, const SIZE: usize, const MAX_WAITERS: usize> {
    pool: &'lock LockPool<T, SIZE, MAX_WAITERS>,
    registered: atomic!(AtomicBool, ty)
}

impl<'lock, T, const SIZE: usize, const MAX_WAITERS: usize> LockFuture<'lock, T, SIZE, MAX_WAITERS> {
    #[cfg(not(loom))]
    pub const fn new(pool: &'lock LockPool<T, SIZE, MAX_WAITERS>) -> Self {
        Self {
            pool,
            registered: atomic!(bool, false)
        }
    }

    #[cfg(loom)]
    pub fn new(pool: &'lock LockPool<T, SIZE, MAX_WAITERS>) -> Self {
        Self {
            pool,
            registered: atomic!(bool, false)
        }
    }
}

impl<'lock, T, const SIZE: usize, const MAX_WAITERS: usize> Future for LockFuture<'lock, T, SIZE, MAX_WAITERS> {
    type Output = LockGuard<'lock, T, SIZE, MAX_WAITERS>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Some(guard) = self.pool.try_get() else {
            if !self.registered.load(ordering!(Relaxed)) {
                match self.pool.waiter_queue.push(unsafe { Waiter::new(cx.waker().clone(), &mut self.registered) }) {
                    Ok(()) => self.registered.store(true, ordering!(Relaxed)),
                    Err(waiter) => waiter.inner_wake()
                }
            }

            return Poll::Pending;
        };

        Poll::Ready(guard)
    }
}