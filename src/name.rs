//! The rule for names: symbols, expressions and operators are registered under names, and
//! the same names are written in templates. A name matches `[a-z_][a-z0-9_]*`.

/// The rule, as it is written in messages.
pub(crate) const PATTERN: &str = "[a-z_][a-z0-9_]*";

pub(crate) fn is_start(byte: u8) -> bool {
    matches!(byte, b'a'..=b'z' | b'_')
}

pub(crate) fn is_continue(byte: u8) -> bool {
    is_start(byte) || byte.is_ascii_digit()
}

pub(crate) fn is_valid(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.first().is_some_and(|&b| is_start(b)) && bytes[1..].iter().all(|&b| is_continue(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_names() {
        for name in ["a", "_", "_x", "u_i", "a1", "velocity_km1", "x2y"] {
            assert!(is_valid(name), "{name:?}");
        }
    }

    #[test]
    fn invalid_names() {
        for name in ["", "A", "1a", "a-b", "a b", "a.b", "é", "aé", "Bad Name"] {
            assert!(!is_valid(name), "{name:?}");
        }
    }
}
