//! Minimal stderr logging matching the `level: message` format used by vm.

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => { eprintln!("info: {}", format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => { eprintln!("warning: {}", format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => { eprintln!("error: {}", format_args!($($arg)*)) };
}
