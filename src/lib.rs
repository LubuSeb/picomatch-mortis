#![forbid(unsafe_code)]

//! A behavior-first Rust port of `micromatch/picomatch`.

/// Returns whether a path exactly equals a literal pattern.
///
/// This deliberately small bootstrap slice is replaced incrementally as each
/// glob construct gains upstream-test proof.
#[must_use]
pub fn is_literal_match(path: &str, pattern: &str) -> bool {
    path == pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_with_auditable_literal_behavior() {
        assert!(is_literal_match("src/lib.rs", "src/lib.rs"));
        assert!(!is_literal_match("src/main.rs", "src/lib.rs"));
    }
}
