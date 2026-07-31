#![forbid(unsafe_code)]

//! A behavior-first Rust port of `micromatch/picomatch`.

mod scan;

pub use scan::{ScanDepth, ScanOptions, ScanState, ScanToken, scan};
