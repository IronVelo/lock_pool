use criterion::{Criterion, criterion_group, criterion_main, black_box};
use lock_pool::LockPool;
use ref_count::futures::MaybeFuture;

fn get(c: &mut Criterion) {
    let pool = LockPool::<usize, 8, 8>::from_fn(|i| i);

    c.bench_function("get", |b| {
        b.iter(|| {
            let out = match pool.get() {
                MaybeFuture::Ready(r) => r,
                MaybeFuture::Future(_f) => unsafe { core::hint::unreachable_unchecked() }
            };
            black_box(out);
        })
    });
}

criterion_group!(benches, get);
criterion_main!(benches);