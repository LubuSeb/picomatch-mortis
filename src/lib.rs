#![forbid(unsafe_code)]

//! A behavior-first Rust port of `micromatch/picomatch`.

mod glob;
mod scan;

pub use glob::{GlobError, GlobOptions, GlobPattern, ParseToken, basename, is_match, parse_tokens};
pub use scan::{ScanDepth, ScanOptions, ScanState, ScanToken, scan};
