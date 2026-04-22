#[macro_export]
macro_rules! Err {
    ($($arg:tt)*) => (::std::result::Result::Err(::anyhow::anyhow!($($arg)*)))
}