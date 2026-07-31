#![forbid(unsafe_code)]

//! A behavior-first Rust port of `micromatch/picomatch`.

mod glob;
mod scan;

pub use glob::{GlobError, GlobOptions, GlobPattern, is_match};
pub use scan::{ScanDepth, ScanOptions, ScanState, ScanToken, scan};
