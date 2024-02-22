use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;
use criterion::{black_box, Criterion, criterion_group, criterion_main};
use object_pool::Pool;
use lock_pool::LockPool;

const NUM_THREADS: usize = 8;
const POOL_SIZE: usize = 16;

pub fn contention(c: &mut Criterion) {
    let mut group = c.benchmark_group("contention performance");

    group.bench_function("try_get contention", |b| {
        b.iter_custom(|iters| {
            let pool = Arc::new(LockPool::<usize, POOL_SIZE, NUM_THREADS>::from_fn(|i| i));
            let barrier = Arc::new(Barrier::new(NUM_THREADS + 1));

            let mut handles = vec![];

            for _ in 0..NUM_THREADS {
                let pool = Arc::clone(&pool);
                let barrier = Arc::clone(&barrier);

                let handle = thread::spawn(move || {
                    barrier.wait();
                    let mut elapsed = Duration::new(0, 0);
                    for _ in 0..iters {
                        let now = std::time::Instant::now();
                        match pool.try_get() {
                            Some(g) => drop(black_box(g)),
                            None => {}
                        }
                        elapsed += now.elapsed();
                    }
                    elapsed
                });

                handles.push(handle);
            }

            barrier.wait();

            let mut total = Duration::new(0, 0);

            for handle in handles {
                total += handle.join().unwrap();
            }

            total / NUM_THREADS as u32
        });
    });

    group.bench_function("object-pool try_pull contention", |b| {
        b.iter_custom(|iters| {
            let pool = Arc::new(Pool::new(POOL_SIZE, || 0));
            let barrier = Arc::new(Barrier::new(NUM_THREADS + 1));

            let mut handles = vec![];

            for _ in 0..NUM_THREADS {
                let pool = Arc::clone(&pool);
                let barrier = Arc::clone(&barrier);

                let handle = thread::spawn(move || {
                    barrier.wait();
                    let mut elapsed = Duration::new(0, 0);
                    for _ in 0..iters {
                        let now = std::time::Instant::now();
                        match pool.try_pull() {
                            Some(e) => drop(black_box(e)),
                            None => {}
                        }
                        elapsed += now.elapsed();
                    }
                    elapsed
                });

                handles.push(handle);
            }

            barrier.wait();
            let mut total = Duration::new(0, 0);

            for handle in handles {
                total += handle.join().unwrap();
            }

            total / NUM_THREADS as u32
        });
    });
}

criterion_group!(benches, contention);
criterion_main!(benches);