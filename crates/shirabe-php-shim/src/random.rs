//! PHP's `random_int()` and `random_bytes()` are cryptographically secure;
//! these are not. Composer does not rely on that property, so a
//! non-cryptographic PRNG is sufficient here.

pub fn random_int<T: RandomInt>(range: impl std::ops::RangeBounds<T>) -> T {
    T::random_int(range)
}

pub fn random_bytes(len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    fastrand::fill(&mut buf);
    buf
}

/// Integral types for which [`random_int`] can generate a value.
pub trait RandomInt: Sized {
    fn random_int(range: impl std::ops::RangeBounds<Self>) -> Self;
}

macro_rules! impl_random_int {
    ($($t:ident),* $(,)?) => {
        $(
            impl RandomInt for $t {
                fn random_int(range: impl std::ops::RangeBounds<Self>) -> Self {
                    fastrand::$t(range)
                }
            }
        )*
    };
}

impl_random_int!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);
