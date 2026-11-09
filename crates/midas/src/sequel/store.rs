//! Migration metadata storage and advisory locking abstractions.

use std::collections::BTreeSet;
use std::time::Duration;

use crate::migration::{
  ChecksumMode,
  MigrationRecord,
};
use crate::Result;

/// Column inventory for a metadata table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetadataInfo {
  /// Whether the table exists.
  pub exists: bool,
  /// Table columns.
  pub columns: BTreeSet<String>,
  /// Whether the table is SQLx-managed (it has no `down_checksum` column).
  pub sqlx_managed: bool,
}

impl MetadataInfo {
  /// Whether the table stores DOWN checksums.
  pub fn has_down_checksum(&self) -> bool {
    self.columns.contains("down_checksum")
  }
}

/// Persistent metadata table operations.
pub trait MigrationStore {
  /// Create the metadata table when it does not exist and upgrade legacy
  /// schemas in place.
  fn ensure_table(&mut self) -> Result<()>;

  /// Describe the configured metadata table without modifying it.
  fn metadata_info(&mut self) -> Result<MetadataInfo> {
    Ok(MetadataInfo::default())
  }

  /// Checksum contract used by the configured metadata table.
  fn checksum_mode(&self) -> ChecksumMode {
    ChecksumMode::Midas
  }

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
