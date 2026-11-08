//! In-memory backend used by unit tests.

use std::collections::{
  BTreeMap,
  BTreeSet,
};
use std::time::Duration;

use super::admin::DatabaseAdmin;
use super::dialect::{
  emit_literal_with,
  quote_ident_with,
  quote_literal_with,
  Dialect,
  Feature,
  SqlLiteral,
};
use super::executor::Executor;
use super::introspect::{
  Introspector,
  SchemaSnapshot,
};
use super::store::{
  AppliedMigration,
  Locker,
  MigrationStore,
};
use crate::Result;

/// In-memory migration metadata store.
#[derive(Debug, Default)]
pub struct MemoryStore {
  applied: BTreeMap<i64, AppliedMigration>,
}

impl MemoryStore {
  /// Snapshot of stored rows ordered by version.
  pub fn snapshot(&self) -> Vec<AppliedMigration> {
    self.applied.values().cloned().collect()
  }

  /// Report whether a version is recorded.
  pub fn contains(&self, version: i64) -> bool {
    self.applied.contains_key(&version)
  }
}

impl MigrationStore for MemoryStore {
  fn ensure_table(&mut self) -> Result<()> {
    Ok(())
  }

  fn list_applied(&mut self) -> Result<Vec<AppliedMigration>> {
    Ok(self.snapshot())
  }

  fn get_last(&mut self) -> Result<Option<AppliedMigration>> {
    Ok(self.applied.values().next_back().cloned())
  }

  fn insert(&mut self, migration: &AppliedMigration) -> Result<()> {
    self.applied.insert(migration.version, migration.clone());
    Ok(())
  }

  fn delete(&mut self, version: i64) -> Result<()> {
    self.applied.remove(&version);
    Ok(())
  }

  fn mark_failed(&mut self, version: i64) -> Result<()> {
    if let Some(record) = self.applied.get_mut(&version) {
      record.success = false;
    }
    Ok(())
  }

  fn repair_checksum(&mut self, version: i64, checksum: &str) -> Result<()> {
    if let Some(record) = self.applied.get_mut(&version) {
      record.checksum = checksum.to_string();
    }
    Ok(())
  }
}

/// In-memory statement executor.
#[derive(Debug, Default)]
pub struct MemoryExecutor {
  statements: Vec<String>,
  batches: Vec<String>,
  in_transaction: bool,
  commits: usize,
  rollbacks: usize,
}

impl MemoryExecutor {
  /// Statements executed individually.
  pub fn statements(&self) -> &[String] {
    &self.statements
  }

  /// Batches executed in one call.
  pub fn batches(&self) -> &[String] {
    &self.batches
  }

  /// Whether a transaction is currently open.
  pub fn in_transaction(&self) -> bool {
    self.in_transaction
  }

  /// Number of successful commits.
  pub fn commits(&self) -> usize {
    self.commits
  }

  /// Number of rollbacks.
  pub fn rollbacks(&self) -> usize {
    self.rollbacks
  }
}

impl Executor for MemoryExecutor {
  fn execute(&mut self, sql: &str) -> Result<()> {
    self.statements.push(sql.to_string());
    Ok(())
  }

  fn execute_batch(&mut self, sql: &str) -> Result<()> {
    self.batches.push(sql.to_string());
    Ok(())
  }

  fn begin(&mut self) -> Result<()> {
    self.in_transaction = true;
    Ok(())
  }

  fn commit(&mut self) -> Result<()> {
    if self.in_transaction {
      self.in_transaction = false;
      self.commits += 1;
    }
    Ok(())
  }

  fn rollback(&mut self) -> Result<()> {
    if self.in_transaction {
      self.in_transaction = false;
      self.rollbacks += 1;
    }
    Ok(())
  }
}

/// Composite in-memory backend implementing every capability trait.
#[derive(Debug)]
pub struct MemoryBackend {
  store: MemoryStore,
  executor: MemoryExecutor,
  databases: BTreeSet<String>,
  locked: bool,
  lock_timeout: Duration,
}

impl Default for MemoryBackend {
  fn default() -> Self {
    Self {
      store: MemoryStore::default(),
      executor: MemoryExecutor::default(),
      databases: BTreeSet::new(),
      locked: false,
      lock_timeout: Duration::from_secs(1),
    }
  }
}

impl MemoryBackend {
  /// Create an empty backend.
  pub fn new() -> Self {
    Self::default()
  }

  /// Access the metadata store.
  pub fn store(&self) -> &MemoryStore {
    &self.store
  }

  /// Access the statement executor.
  pub fn executor(&self) -> &MemoryExecutor {
    &self.executor
  }

  /// Report whether the migrator lock is held.
  pub fn is_locked(&self) -> bool {
    self.locked
  }

  /// Databases known to this backend.
  pub fn databases(&self) -> &BTreeSet<String> {
    &self.databases
  }
}

impl Executor for MemoryBackend {
  fn execute(&mut self, sql: &str) -> Result<()> {
    self.executor.execute(sql)
  }

  fn execute_batch(&mut self, sql: &str) -> Result<()> {
    self.executor.execute_batch(sql)
  }

  fn begin(&mut self) -> Result<()> {
    self.executor.begin()
  }

  fn commit(&mut self) -> Result<()> {
    self.executor.commit()
  }

  fn rollback(&mut self) -> Result<()> {
    self.executor.rollback()
  }
}

impl MigrationStore for MemoryBackend {
  fn ensure_table(&mut self) -> Result<()> {
    self.store.ensure_table()
  }

  fn list_applied(&mut self) -> Result<Vec<AppliedMigration>> {
    self.store.list_applied()
  }

  fn get_last(&mut self) -> Result<Option<AppliedMigration>> {
    self.store.get_last()
  }

  fn insert(&mut self, migration: &AppliedMigration) -> Result<()> {
    self.store.insert(migration)
  }

  fn delete(&mut self, version: i64) -> Result<()> {
    self.store.delete(version)
  }

  fn mark_failed(&mut self, version: i64) -> Result<()> {
    self.store.mark_failed(version)
  }

  fn repair_checksum(&mut self, version: i64, checksum: &str) -> Result<()> {
    self.store.repair_checksum(version, checksum)
  }
}

impl Locker for MemoryBackend {
  fn lock(&mut self) -> Result<()> {
    self.locked = true;
    Ok(())
  }

  fn unlock(&mut self) -> Result<()> {
    self.locked = false;
    Ok(())
  }

  fn lock_timeout(&self) -> Duration {
    self.lock_timeout
  }
}

impl Dialect for MemoryBackend {
  fn quote_ident(&self, ident: &str) -> String {
    quote_ident_with(ident, '"')
  }

  fn quote_literal(&self, value: &str) -> String {
    quote_literal_with(value, '\'')
  }

  fn emit_literal(&self, value: &SqlLiteral<'_>) -> String {
    emit_literal_with(value, '\'')
  }

  fn supports(&self, feature: Feature) -> bool {
    matches!(
      feature,
      Feature::Schemas | Feature::TransactionalDdl | Feature::MultiStatementBatch
    )
  }
}

impl Introspector for MemoryBackend {
  fn introspect_schema(&mut self, schema: &str) -> Result<SchemaSnapshot> {
    Ok(SchemaSnapshot {
      schema: schema.to_string(),
      objects: Vec::new(),
    })
  }

  fn list_schemas(&mut self) -> Result<Vec<String>> {
    Ok(self.databases.iter().cloned().collect())
  }
}

impl DatabaseAdmin for MemoryBackend {
  fn create_database(&mut self, name: &str) -> Result<()> {
    self.databases.insert(name.to_string());
    Ok(())
  }

  fn drop_database(&mut self, name: &str) -> Result<()> {
    self.databases.remove(name);
    Ok(())
  }

  fn database_exists(&mut self, name: &str) -> Result<bool> {
    Ok(self.databases.contains(name))
  }

  fn force_drop_database(&mut self, name: &str) -> Result<()> {
    self.drop_database(name)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn store_tracks_applied_migrations() {
    let mut store = MemoryStore::default();
    store
      .insert(&AppliedMigration::applied(1, "init", "checksum"))
      .expect("insert");
    assert!(store.contains(1));
    assert_eq!(store.get_last().expect("last").map(|row| row.version), Some(1));

    store.mark_failed(1).expect("mark failed");
    assert!(!store.snapshot()[0].success);

    store.repair_checksum(1, "new").expect("repair");
    assert_eq!(store.snapshot()[0].checksum, "new");

    store.delete(1).expect("delete");
    assert!(!store.contains(1));
  }

  #[test]
  fn executor_tracks_transactions() {
    let mut executor = MemoryExecutor::default();
    executor.begin().expect("begin");
    assert!(executor.in_transaction());
    executor.commit().expect("commit");
    assert!(!executor.in_transaction());
    assert_eq!(executor.commits(), 1);

    executor.begin().expect("begin");
    executor.rollback().expect("rollback");
    assert_eq!(executor.rollbacks(), 1);
  }

  #[test]
  fn backend_supports_lifecycle() {
    let mut backend = MemoryBackend::new();
    backend.lock().expect("lock");
    assert!(backend.is_locked());
    backend.unlock().expect("unlock");
    assert!(!backend.is_locked());

    backend.create_database("app").expect("create");
    assert!(backend.database_exists("app").expect("exists"));
    backend.force_drop_database("app").expect("drop");
    assert!(!backend.database_exists("app").expect("exists"));
  }
}
