//! Database lifecycle operations.

use crate::Result;

/// Create, inspect, and remove databases.
pub trait DatabaseAdmin {
  /// Create a database.
  fn create_database(&mut self, name: &str) -> Result<()>;

  /// Drop a database.
  fn drop_database(&mut self, name: &str) -> Result<()>;

  /// Report whether a database exists.
  fn database_exists(&mut self, name: &str) -> Result<bool>;

  /// Drop a database even when sessions are still connected.
  fn force_drop_database(&mut self, name: &str) -> Result<()>;
}
