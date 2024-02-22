
use criterion::{Criterion, criterion_group, criterion_main, black_box};
use lock_pool::LockPool;

fn try_get(c: &mut Criterion) {
    let pool = LockPool::<usize, 8, 8>::from_fn(|i| i);

    c.bench_function("try_get", |b| {
        b.iter(|| {
            let out = pool.try_get().unwrap();
            black_box(out);
        })
    });
}


criterion_group!(benches, try_get);
criterion_main!(benches);