//! Per-migration directives.
//!
//! A directive is a `-- shki:<name>` comment line anywhere in a migration file
//! that changes how shki executes that file. Directives live in comments, and
//! checksums are computed on comment-stripped SQL (see
//! [`crate::migrate::checksum`]), so adding one to an already-applied migration
//! never invalidates the journal — execution semantics are deliberately not
//! checksummed.
//!
//! To add a directive: add a field to [`Directives`], list its name in
//! [`Directives::KNOWN`], and set the field from the match in
//! [`Directives::parse`].

use crate::{Result, ShkiError};

/// Marker that distinguishes a shki directive from an ordinary SQL comment.
pub const DIRECTIVE_PREFIX: &str = "shki:";

/// The directive line generators write to opt a migration out of the wrapping
/// transaction. Kept next to [`Directives::parse`] so writer and reader can't
/// drift; the round-trip is asserted in this module's tests.
pub const NO_TRANSACTION_DIRECTIVE: &str = "-- shki:no-transaction";

/// Execution options parsed from a migration file's comments.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Directives {
    /// `-- shki:no-transaction` — run this migration outside shki's wrapping
    /// transaction, one statement segment at a time.
    ///
    /// Required for statements Postgres refuses inside a transaction block,
    /// notably `CREATE INDEX CONCURRENTLY`. Such migrations must be idempotent:
    /// a mid-file failure leaves earlier segments committed and the migration
    /// unrecorded, so the next run replays it from the top.
    pub no_transaction: bool,
}

impl Directives {
    /// Every recognised directive name.
    pub const KNOWN: &'static [&'static str] = &["no-transaction"];

    /// Parse directives from migration SQL.
    ///
    /// An unrecognised `shki:` directive is an error rather than a silent
    /// no-op: a typo'd `-- shki:no-transactions` would otherwise be discovered
    /// only when the migration fails against production.
    pub fn parse(sql: &str) -> Result<Self> {
        let mut directives = Self::default();

        for name in directive_names(sql) {
            match name.as_str() {
                "no-transaction" => directives.no_transaction = true,
                unknown => {
                    return Err(ShkiError::migration(format!(
                        "Unknown migration directive '-- {DIRECTIVE_PREFIX}{unknown}'. \
                         Known directives: {}",
                        Self::KNOWN.join(", ")
                    )));
                }
            }
        }

        Ok(directives)
    }
}

/// Yield the name from every `-- shki:<name>` comment line, case-insensitively.
fn directive_names(sql: &str) -> impl Iterator<Item = String> + '_ {
    sql.lines().filter_map(|line| {
        let comment = line.trim().strip_prefix("--")?.trim().to_ascii_lowercase();
        Some(comment.strip_prefix(DIRECTIVE_PREFIX)?.trim().to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrate::checksum::sql_checksum;

    #[test]
    fn absent_directive_defaults_to_transactional() {
        let directives = Directives::parse("CREATE TABLE t (id INT);").expect("should parse");
        assert_eq!(directives, Directives::default());
        assert!(!directives.no_transaction);
    }

    #[test]
    fn no_transaction_constant_round_trips_through_parse() {
        let sql = format!("{NO_TRANSACTION_DIRECTIVE}\nCREATE INDEX CONCURRENTLY i ON t (c);");
        assert!(Directives::parse(&sql).expect("should parse").no_transaction);
    }

    #[test]
    fn no_transaction_directive_is_recognised() {
        let sql = "-- Migration: 0009_index (up)\n-- shki:no-transaction\nCREATE INDEX CONCURRENTLY i ON t (c);";
        assert!(Directives::parse(sql).expect("should parse").no_transaction);
    }

    #[test]
    fn directive_tolerates_spacing_and_case() {
        for line in [
            "--shki:no-transaction",
            "--   SHKI: No-Transaction   ",
            "\t-- Shki:no-transaction",
        ] {
            assert!(
                Directives::parse(&format!("{line}\nSELECT 1;"))
                    .expect("should parse")
                    .no_transaction,
                "expected {line:?} to be recognised"
            );
        }
    }

    #[test]
    fn ordinary_comments_are_not_directives() {
        let sql = "-- shki is great\n-- no-transaction\nSELECT 1;";
        assert!(!Directives::parse(sql).expect("should parse").no_transaction);
    }

    #[test]
    fn unknown_directive_is_rejected() {
        let error = Directives::parse("-- shki:no-transactions\nSELECT 1;")
            .expect_err("typo should be rejected");
        let message = error.to_string();
        assert!(message.contains("no-transactions"), "{message}");
        assert!(message.contains("no-transaction"), "{message}");
    }

    #[test]
    fn adding_a_directive_does_not_change_the_checksum() {
        let plain = "CREATE INDEX i ON t (c);";
        let annotated = "-- shki:no-transaction\nCREATE INDEX i ON t (c);";
        assert_eq!(sql_checksum(plain), sql_checksum(annotated));
    }
}
