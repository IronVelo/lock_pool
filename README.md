# LockPool

[![CI](https://github.com/IronVelo/lock_pool/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/IronVelo/lock_pool/actions/workflows/ci.yml)

Designed to meet the stringent requirements of high-performance, real-time systems, LockPool is a high-throughput, fair,
and thread-safe asynchronous object pool for Rust, offering unparalleled performance in environments where efficiency 
and low latency are crucial. Its architecture is specifically tailored to operate flawlessly in a wide range of 
environments, from the constrained realms of bare-metal systems such as seL4 to the expansive ecosystems of operating 
systems like Linux, without making any assumptions about the underlying asynchronous runtime. By leveraging core futures
and wakers, LockPool integrates effortlessly with any asynchronous ecosystem, maintaining runtime agnosticism and 
ensuring wide applicability.

Engineered with no_std and no_alloc constraints at its core, LockPool is an ideal solution for embedded systems and 
other memory-constrained environments where dynamic memory allocation is either undesirable or unavailable. This focus 
on minimalism and efficiency ensures that LockPool remains lightweight and versatile, suitable for a broad spectrum of 
use cases.

### How It Works

The `LockPool` distinguishes itself not by merely utilizing synchronization mechanisms but by embodying the principle 
of synchronization within its architecture. At its core, the pool is composed of lightweight mutexes that, in isolation,
provide only the basic guarantee of mutual exclusion. However, the true innovation of `LockPool` lies in its 
sophisticated management of these mutexes, which orchestrates access to the pooled resources.

This orchestration is twofold:

1. *Contention Management*: Unlike traditional approaches that rely on complex locking schemes, `LockPool` employs a 
direct and efficient method to manage access to its resources. Each mutex within the pool operates with minimal 
overhead, allowing threads to compete for access without the common pitfalls of lock contention. This is especially 
critical in high-performance and real-time systems, where minimizing latency is paramount.

2. *Fairness and Efficiency*: Fairness is ensured through a lock-free queue for managing threads that are waiting for 
an object. This queue prevents priority inversion and ensures that waiting threads are granted access in a timely and 
predictable manner. By decoupling the waiting mechanism from the mutexes, `LockPool` achieves a level of fairness and 
efficiency in resource distribution that is seldom seen in conventional locking strategies.

In essence, `LockPool` transforms the concept of synchronization from a potential bottleneck into a dynamic, efficient, 
and fair system for resource management. Its ability to distribute contention across objects and manage waiting threads 
without introducing priority inversion or unnecessary overhead makes it an exemplary solution for applications where 
performance and reliability cannot be compromised.

## Features

- **High Performance Under Contention**: Excelling in high-contention scenarios, LockPool provides efficient access to 
pooled objects, significantly outperforming traditional locking approaches.
- **Fairness and Predictability**: Ensures equitable access through a lock-free queue for waiting threads and a 
contention-aware pool design, making it highly reliable for real-time systems.
- **Environment and Runtime Agnostic**: Compatible across various platforms and asynchronous runtimes, thanks to its 
exclusive reliance on core futures and wakers.
- **no_std & no_alloc Compatibility**: Perfectly suited for systems with strict memory constraints, emphasizing 
efficiency and resource conservation.
- **Thread Safety**: Delivers safe concurrent access to pool objects, ensuring data integrity and system stability.

## Example

```rust
use lock_pool::{LockPool, maybe_await};

async fn example() {
    let pool = LockPool::<usize, 5, 32>::from_fn(|i| i);

    let g0 = maybe_await!(pool.get());
    assert_eq!(*g0, 0);
    let g1 = maybe_await!(pool.get());
    assert_eq!(*g1, 1);
    let g2 = maybe_await!(pool.get());
    assert_eq!(*g2, 2);
    let g3 = maybe_await!(pool.get());
    assert_eq!(*g3, 3);
    let g4 = maybe_await!(pool.get());
    assert_eq!(*g4, 4);

    assert!(pool.try_get().is_none());

    drop(g0);
    assert_eq!(*maybe_await!(pool.get()), 0);
}
```

Embodying the essence of Rust's concurrency model, LockPool is a robust and efficient foundation for building 
high-performance, dependable real-time systems, ensuring that performance and fairness are not mutually exclusive.

### Future Additions

- `get_or_create()`: For either getting or creating an object, making the api simpler for certain connection pooling
                     patterns.

  