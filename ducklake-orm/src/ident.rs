//! Helpers for safely embedding identifiers and string literals into SQL.
//!
//! These helpers exist because DuckDB does not allow bind parameters in every
//! position (notably the `ATTACH … AS <name>` alias and the `AT (TIMESTAMP =>
//! '<literal>')` clause). When we are forced to interpolate, we either:
//!
//! - Validate the value as a strict SQL identifier (`validate_identifier`), or
//! - Escape it as a single-quoted string literal (`escape_sql_string`).
//!
//! Used together, these prevent SQL injection through user-controlled catalog
//! names, timestamps, or any other value that cannot be passed as a `$N` bind
//! parameter.

use crate::error::DuckLakeError;

/// Maximum number of concurrent connections the pool will allow.
///
/// This is a sanity bound to prevent accidental resource exhaustion from a
/// misconfigured `ducklake.toml`. It is intentionally generous — real-world
/// DuckDB read pools rarely exceed a few dozen connections.
pub(crate) const MAX_POOL_SIZE: u32 = 1024;

/// Validate that `name` is a safe SQL identifier suitable for being
/// interpolated directly into a SQL statement (e.g. as a catalog alias, a
/// schema name, or a table name).
///
/// A "safe" identifier matches the regex `^[A-Za-z_][A-Za-z0-9_]*$` and is at
/// most 63 bytes long. This rejects any character that could break out of the
/// surrounding SQL context: whitespace, quotes, semicolons, comments, etc.
///
/// `kind` is included in the error message to identify which identifier was
/// rejected (e.g. `"catalog_name"`, `"schema"`).
pub(crate) fn validate_identifier(name: &str, kind: &str) -> Result<(), DuckLakeError> {
    if name.is_empty() {
        return Err(DuckLakeError::Config(format!(
            "{kind} must not be empty"
        )));
    }
    if name.len() > 63 {
        return Err(DuckLakeError::Config(format!(
            "{kind} '{name}' is too long ({} bytes; maximum is 63)",
            name.len()
        )));
    }
    let valid = name
        .chars()
        .enumerate()
        .all(|(i, c)| {
            let ok = c.is_ascii_alphanumeric() || c == '_';
            ok && (i != 0 || c.is_ascii_alphabetic() || c == '_')
        });
    if !valid {
        return Err(DuckLakeError::Config(format!(
            "{kind} '{name}' is not a valid SQL identifier; \
             only ASCII letters, digits, and underscores are allowed \
             (and it must not start with a digit)"
        )));
    }
    Ok(())
}

/// Escape `s` for safe interpolation into a SQL single-quoted string literal.
///
/// Every `'` becomes `''` and every backslash becomes `\\` (DuckDB accepts
/// the standard SQL doubling rule for `'`; the backslash escape is a
/// defence-in-depth measure for `E'…'` style literals). The returned string
/// is intended to be wrapped in single quotes by the caller, e.g.
/// `format!("ATTACH '{}' AS ...", escape_sql_string(path))`.
pub(crate) fn escape_sql_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\'' => out.push_str("''"),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_identifiers() {
        assert!(validate_identifier("lake", "catalog_name").is_ok());
        assert!(validate_identifier("main", "schema").is_ok());
        assert!(validate_identifier("_foo", "x").is_ok());
        assert!(validate_identifier("abc_123", "x").is_ok());
        assert!(validate_identifier("A", "x").is_ok());
    }

    #[test]
    fn invalid_identifiers() {
        assert!(validate_identifier("", "x").is_err());
        assert!(validate_identifier("1abc", "x").is_err()); // starts with digit
        assert!(validate_identifier("a b", "x").is_err()); // whitespace
        assert!(validate_identifier("a;b", "x").is_err()); // semicolon
        assert!(validate_identifier("a'b", "x").is_err()); // quote
        assert!(validate_identifier("a-b", "x").is_err()); // hyphen
        assert!(validate_identifier("a.b", "x").is_err()); // dot
        assert!(validate_identifier(
            "x); DROP TABLE t; --",
            "x"
        )
        .is_err());
    }

    #[test]
    fn escape_quotes_and_backslashes() {
        assert_eq!(escape_sql_string("simple"), "simple");
        assert_eq!(escape_sql_string("it's"), "it''s");
        assert_eq!(escape_sql_string(r"a\b"), r"a\\b");
        assert_eq!(
            escape_sql_string("x'); DROP TABLE t; --"),
            "x''); DROP TABLE t; --"
        );
    }
}
