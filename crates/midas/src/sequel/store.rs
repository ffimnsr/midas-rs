//! Migration metadata storage and advisory locking abstractions.

use std::time::Duration;

use crate::migration::MigrationRecord;
use crate::Result;

/// Persistent metadata table operations.
pub trait MigrationStore {
  /// Create the metadata table when it does not exist and upgrade legacy
  /// schemas in place.
  fn ensure_table(&mut self) -> Result<()>;

  /// List every metadata row ordered by version.
  fn list_migrations(&mut self) -> Result<Vec<MigrationRecord>>;

  /// Return the most recently recorded migration.
  fn get_last(&mut self) -> Result<Option<MigrationRecord>>;

  /// Record a migration.
  fn insert(&mut self, record: &MigrationRecord) -> Result<()>;

  /// Insert a record or replace the existing row for the same version.
  fn upsert(&mut self, record: &MigrationRecord) -> Result<()>;

  /// Remove a migration record.
  fn delete(&mut self, version: i64) -> Result<()>;

  /// Set the success flag for a recorded migration.
  fn set_success(&mut self, version: i64, success: bool) -> Result<()>;

  /// Update the description and checksums for an applied migration.
  fn repair(&mut self, record: &MigrationRecord) -> Result<()>;

  /// Drop the metadata table.
  fn drop_table(&mut self) -> Result<()>;

  /// Drop and recreate the metadata table.
  fn reset(&mut self) -> Result<()> {
    self.drop_table()?;
    self.ensure_table()
  }
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
