//! Migration metadata storage and advisory locking abstractions.

use std::time::Duration;

use crate::Result;

/// A row in the schema migrations metadata table.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppliedMigration {
  /// Migration version.
  pub version: i64,
  /// Human-readable migration description.
  pub description: String,
  /// SHA-384 checksum over the canonical UP/DOWN SQL.
  pub checksum: String,
  /// Wall-clock execution time in milliseconds.
  pub execution_time_ms: i64,
  /// Whether the migration completed successfully.
  pub success: bool,
}

impl AppliedMigration {
  /// Build a successfully applied migration record.
  pub fn applied(version: i64, description: impl Into<String>, checksum: impl Into<String>) -> Self {
    Self {
      version,
      description: description.into(),
      checksum: checksum.into(),
      execution_time_ms: 0,
      success: true,
    }
  }
}

/// Persistent metadata table operations.
pub trait MigrationStore {
  /// Create the metadata table when it does not exist.
  fn ensure_table(&mut self) -> Result<()>;

  /// List every applied migration ordered by version.
  fn list_applied(&mut self) -> Result<Vec<AppliedMigration>>;

  /// Return the most recently applied migration.
  fn get_last(&mut self) -> Result<Option<AppliedMigration>>;

  /// Record a migration.
  fn insert(&mut self, migration: &AppliedMigration) -> Result<()>;

  /// Remove a migration record.
  fn delete(&mut self, version: i64) -> Result<()>;

  /// Mark a migration as failed without deleting its record.
  fn mark_failed(&mut self, version: i64) -> Result<()>;

  /// Replace the stored checksum for a migration.
  fn repair_checksum(&mut self, version: i64, checksum: &str) -> Result<()>;
}

/// Advisory locking used to serialize concurrent migrators.
pub trait Locker {
  /// Acquire the migrator lock.
  fn lock(&mut self) -> Result<()>;

  /// Release the migrator lock.
  fn unlock(&mut self) -> Result<()>;

  /// Configured lock timeout.
  fn lock_timeout(&self) -> Duration;
}
