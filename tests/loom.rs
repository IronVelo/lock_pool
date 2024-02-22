#[cfg(loom)]
mod loom {
    use lock_pool::{LockPool, maybe_await};
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
}