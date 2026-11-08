//! Migration model.
//!
//! Phase 0 only needs the type referenced by the `midas::migrate!` macro. The
//! migration engine phase expands this module with raw file parsing, metadata,
//! and checksum support.

/// A migration known at compile time or discovered from disk.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Migration {
  /// Migration version.
  pub version: i64,
  /// Human-readable description.
  pub description: String,
  /// UP SQL.
  pub up: String,
  /// DOWN SQL, when the migration is reversible.
  pub down: Option<String>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn migration_defaults_are_empty() {
    let migration = Migration::default();
    assert_eq!(migration.version, 0);
    assert!(migration.description.is_empty());
    assert!(migration.up.is_empty());
    assert!(migration.down.is_none());
  }
}
