#[cfg(loom)]
mod loom {
    use lock_pool::{LockPool, maybe_await, fast_mutex::FastMutex};
    use loom::thread;
    use loom::sync::Arc;
    use loom::future::block_on;

    #[test]
    fn gets() {
        loom::model(|| {
            let pool = Arc::new(LockPool::<usize, 8, 32>::from_fn(|i| i));
            let p = pool.clone();

            let t1 = thread::spawn(move || {
                block_on(async {
                    let mut g = maybe_await!(p.get());
                    *g = 5;
                });
            });

            let p = pool.clone();
            let t2 = thread::spawn(move || {
                block_on(async {
                    let mut g = maybe_await!(p.get());
                    *g = 5;
                });
            });

            t1.join().unwrap();
            t2.join().unwrap();
        })
    }

    #[test]
    fn gets_multi() {
        loom::model(|| {
            let pool = Arc::new(LockPool::<usize, 8, 32>::from_fn(|i| i));
            let p = pool.clone();

            let t1 = thread::spawn(move || {
                block_on(async {
                    let mut g = maybe_await!(p.get());
                    *g = 5;
                    let mut g1 = maybe_await!(p.get());
                    *g1 = 6;
                });
            });

            let p = pool.clone();
            let t2 = thread::spawn(move || {
                block_on(async {
                    let mut g = maybe_await!(p.get());
                    *g = 5;
                    let mut g1 = maybe_await!(p.get());
                    *g1 = 6;
                });
            });

            t1.join().unwrap();
            t2.join().unwrap();
        })
    }

    #[test]
    fn try_gets() {
        loom::model(|| {
            let pool = Arc::new(LockPool::<usize, 8, 32>::from_fn(|i| i));
            let p = pool.clone();

            let t1 = thread::spawn(move || {
                if let Some(mut g) = p.try_get() {
                    *g = 5;
                }
            });

            let p = pool.clone();
            let t2 = thread::spawn(move || {
                if let Some(mut g) = p.try_get() {
                    *g = 5;
                }
            });

            t1.join().unwrap();
            t2.join().unwrap();
        })
    }

    #[test]
    fn try_gets_multi() {
        loom::model(|| {
            let pool = Arc::new(LockPool::<usize, 8, 32>::from_fn(|i| i));
            let p = pool.clone();

            let t1 = thread::spawn(move || {
                let g = p.try_get();
                g.map(|mut guard| *guard = 5);
                let g1 = p.try_get();
                g1.map(|mut guard| *guard = 5);
            });

            let p = pool.clone();
            let t2 = thread::spawn(move || {
                let g = p.try_get();
                g.map(|mut guard| *guard = 5);
                let g1 = p.try_get();
                g1.map(|mut guard| *guard = 5);
            });

            t1.join().unwrap();
            t2.join().unwrap();
        })
    }

    #[test]
    fn try_while_locked_fast_mutex() {
        loom::model(|| {
            let successes_src = Arc::new(core::sync::atomic::AtomicU32::new(0));
            let m = Arc::new(FastMutex::new(0));
            let mc = m.clone();
            let successes = successes_src.clone();
            let t1 = thread::spawn(move || {
                successes.fetch_add(mc.try_while_locked(|v| {*v += 1}).is_some() as u32, core::sync::atomic::Ordering::AcqRel);
            });

            let mc = m.clone();
            let successes = successes_src.clone();
            let t2 = thread::spawn(move || {
                successes.fetch_add(mc.try_while_locked(|v| {*v += 1}).is_some() as u32, core::sync::atomic::Ordering::AcqRel);
            });

            t1.join().unwrap();
            t2.join().unwrap();

            let successes = successes_src.load(core::sync::atomic::Ordering::Acquire);
            assert_eq!(
                unsafe { m.peek(|v| *v) }, successes
            );
        })
    }
}