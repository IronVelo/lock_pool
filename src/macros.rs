#[cfg(loom)]
macro_rules! ordering {
    (ty) => {
        ::loom::sync::atomic::Ordering
    };
    ($order:ident) => {
        ::loom::sync::atomic::Ordering::$order
    };
}

#[cfg(not(loom))]
macro_rules! ordering {
    (ty) => {
        ::core::sync::atomic::Ordering
    };
    ($order:ident) => {
        ::core::sync::atomic::Ordering::$order
    };
}

#[cfg(loom)]
macro_rules! atomic {
    (bool, $value:expr) => {
        ::loom::sync::atomic::AtomicBool::new($value)
    };
    (u8, $value:expr) => {
        ::loom::sync::atomic::AtomicU8::new($value)
    };
    (u16, $value:expr) => {
        ::loom::sync::atomic::AtomicU16::new($value)
    };
    (u32, $value:expr) => {
        ::loom::sync::atomic::AtomicU32::new($value)
    };
    (u64, $value:expr) => {
        ::loom::sync::atomic::AtomicU64::new($value)
    };
    (usize, $value:expr) => {
        ::loom::sync::atomic::AtomicUsize::new($value)
    };
    ($kind:ident) => {
        atomic!($kind, 0)
    };
    ($kind:ident, ty) => {
        ::loom::sync::atomic::$kind
    };
}

#[cfg(not(loom))]
macro_rules! atomic {
    (bool, $value:expr) => {
        ::core::sync::atomic::AtomicBool::new($value)
    };
    (u8, $value:expr) => {
        ::core::sync::atomic::AtomicU8::new($value)
    };
    (u16, $value:expr) => {
        ::core::sync::atomic::AtomicU16::new($value)
    };
    (u32, $value:expr) => {
        ::core::sync::atomic::AtomicU32::new($value)
    };
    (u64, $value:expr) => {
        ::core::sync::atomic::AtomicU64::new($value)
    };
    (usize, $value:expr) => {
        ::core::sync::atomic::AtomicUsize::new($value)
    };
    ($kind:ident) => {
        atomic!($kind, 0)
    };
    ($kind:ident, ty) => {
        ::core::sync::atomic::$kind
    };
}