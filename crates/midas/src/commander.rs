//! Migration runner.

use std::collections::BTreeSet;
use std::time::Instant;

use console::style;
use prettytable::format::consts;
use prettytable::{
  color,
  row,
  Attr,
  Cell,
  Row,
  Table,
};
use serde::Serialize;

use crate::config::TransactionMode;
use crate::migration::{
  checksum_hex,
  ChecksumMode,
  LockGuard,
  Migration,
  MigrationList,
  MigrationRecord,
};
use crate::sequel::{
  Backend,
  Feature,
};
use crate::{
  Error,
  Result,
};

/// Runner behavior controlled by CLI flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MigratorOptions {
  /// Print SQL without executing it.
  pub dry_run: bool,
  /// Skip advisory locking.
  pub no_lock: bool,
  /// Transaction policy for migrations that do not opt out.
  pub transaction: TransactionMode,
  /// Suppress human-readable progress output (used by `--json`).
  pub quiet: bool,
  /// Ignore applied versions whose migration file is no longer present.
  pub ignore_missing: bool,
}

/// Summary state used by `info` and JSON output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MigrationInfo {
  /// JSON schema version.
  pub version: u32,
  /// Successfully applied versions.
  pub applied: Vec<i64>,
  /// Versions present on disk but not applied.
  pub pending: Vec<i64>,
  /// Lowest failed version, if the database is dirty.
  pub dirty: Option<i64>,
}

/// Coordinates migration files with a database backend.
pub struct Migrator {
  backend: Box<dyn Backend>,
  migrations: MigrationList,
  options: MigratorOptions,
}

impl Migrator {
  /// Create a migrator from a backend and migration set.
  pub fn new(backend: Box<dyn Backend>, migrations: MigrationList) -> Self {
    Self {
      backend,
      migrations,
      options: MigratorOptions::default(),
    }
  }

  /// Replace runner options.
  pub fn with_options(mut self, options: MigratorOptions) -> Self {
    self.options = options;
    self
  }

  /// Return the current migration state.
  pub fn info(&mut self) -> Result<MigrationInfo> {
    let records = self.backend.list_migrations()?;
    Ok(build_info(&records, &self.migrations))
  }

  /// Print the current migration status.
  pub fn status(&mut self) -> Result<MigrationInfo> {
    let info = self.info()?;
    if self.migrations.is_empty() {
      println!("There are no available migration files.");
      return Ok(info);
    }

    let records = self.backend.list_migrations()?;
    let dirty = info.dirty;
    let mut table = Table::new();
    table.set_titles(row![Fbb->"Migration No.", Fbb->"Status", Fbb->"Filename", Fbb->"Description"]);
    table.set_format(*consts::FORMAT_CLEAN);

    for migration in self.migrations.values() {
      let record = records.iter().find(|record| record.version == migration.version);
      let (label, temp_color) = match record {
        Some(record) if record.success => ("Active", color::GREEN),
        Some(_) => ("Dirty", color::RED),
        None => ("Pending", color::YELLOW),
      };
      table.add_row(Row::new(vec![
        Cell::new(&format!("{:013}", migration.version)).with_style(Attr::Bold),
        Cell::new(label).with_style(Attr::ForegroundColor(temp_color)),
        Cell::new(
          &migration
            .source_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
        )
        .with_style(Attr::ForegroundColor(temp_color)),
        Cell::new(&migration.description).with_style(Attr::ForegroundColor(temp_color)),
      ]));
    }

    let msg = style("Available migrations:").bold().cyan();
    println!();
    println!("{msg}");
    println!();
    table.printstd();
    println!();
    println!(
      "{}: {}",
      style("Completed migrations").bold().cyan(),
      info.applied.len()
    );
    println!(
      "{}: {}",
      style("Pending migrations").bold().cyan(),
      info.pending.len()
    );
    if let Some(version) = dirty {
      println!("{}: {version:013}", style("Dirty migration").bold().red());
    }
    println!();

    Ok(info)
  }

  /// Apply every pending migration, optionally up to `target`.
  pub fn up(&mut self, target: Option<i64>) -> Result<MigrationInfo> {
    self.with_backend(|backend, migrations, options| {
      let records = backend.list_migrations()?;
      ensure_not_dirty(&records)?;
      let mode = backend.checksum_mode();
      validate_migrations(&records, migrations, &mode, options)?;

      let pending = pending_versions(&records, migrations, target);
      for version in pending {
        let migration = migrations
          .get(&version)
          .ok_or(Error::MigrationNotFound { version })?;
        apply_migration(backend, migration, options)?;
      }

      Ok(build_info(&backend.list_migrations()?, migrations))
    })
  }

  /// Revert every applied migration. Requires `force`.
  pub fn down(&mut self, force: bool) -> Result<MigrationInfo> {
    if !force {
      return Err(Error::Migration(
        "refusing to revert every migration without `--force`".to_string(),
      ));
    }
    self.revert_to(Some(0))
  }

  /// Revert applied migrations above `target`. `None` reverts the last one.
  pub fn revert_to(&mut self, target: Option<i64>) -> Result<MigrationInfo> {
    self.with_backend(|backend, migrations, options| {
      let records = backend.list_migrations()?;
      ensure_not_dirty(&records)?;
      let mode = backend.checksum_mode();
      validate_migrations(&records, migrations, &mode, options)?;

      let mut versions = records
        .iter()
        .filter(|record| record.success)
        .filter(|record| target.is_none_or(|target| record.version > target))
        .map(|record| record.version)
        .collect::<Vec<_>>();
      versions.sort_unstable();
      versions.reverse();
      if target.is_none() {
        versions.truncate(1);
      }

      for version in versions {
        let migration = migrations
          .get(&version)
          .ok_or(Error::MigrationNotFound { version })?;
        revert_migration(backend, migration, options)?;
      }

      Ok(build_info(&backend.list_migrations()?, migrations))
    })
  }

  /// Revert and re-apply the last applied migration.
  pub fn redo(&mut self) -> Result<MigrationInfo> {
    let last = {
      let records = self.backend.list_migrations()?;
      ensure_not_dirty(&records)?;
      records
        .iter()
        .filter(|record| record.success)
        .map(|record| record.version)
        .max()
    };

    let Some(version) = last else {
      if !self.options.quiet {
        println!("Migrations table is empty. No need to redo migrations.");
      }
      return self.info();
    };

    self.with_backend(|backend, migrations, options| {
      let migration = migrations
        .get(&version)
        .ok_or(Error::MigrationNotFound { version })?;
      revert_migration(backend, migration, options)?;
      apply_migration(backend, migration, options)?;
      Ok(build_info(&backend.list_migrations()?, migrations))
    })
  }

  /// Mark migrations up to `target` as applied without executing SQL.
  pub fn baseline(&mut self, target: i64) -> Result<MigrationInfo> {
    self.with_backend(|backend, migrations, options| {
      for migration in migrations
        .values()
        .filter(|migration| migration.version <= target)
      {
        if options.dry_run {
          println!("-- baseline {:013} {}", migration.version, migration.description);
          continue;
        }
        let record = record_for(migration, 0, true, &backend.checksum_mode());
        write_metadata(backend, &record, true)?;
      }
      Ok(build_info(&backend.list_migrations()?, migrations))
    })
  }

  /// Recompute checksums and descriptions for applied migrations.
  pub fn repair(&mut self) -> Result<MigrationInfo> {
    self.with_backend(|backend, migrations, _| {
      let records = backend.list_migrations()?;
      let mode = backend.checksum_mode();
      for record in records.iter().filter(|record| record.success) {
        let Some(migration) = migrations.get(&record.version) else {
          continue;
        };
        let repaired = record_for(migration, record.execution_time, true, &mode);
        backend.repair(&repaired)?;
      }
      Ok(build_info(&backend.list_migrations()?, migrations))
    })
  }

  /// Mark a dirty migration as applied.
  pub fn resolve_applied(&mut self, version: Option<i64>) -> Result<MigrationInfo> {
    self.with_backend(|backend, migrations, _| {
      let records = backend.list_migrations()?;
      let version = resolve_version(&records, version)?;
      backend.set_success(version, true)?;
      Ok(build_info(&backend.list_migrations()?, migrations))
    })
  }

  /// Remove a dirty migration record so it can run again.
  pub fn resolve_rolled_back(&mut self, version: Option<i64>) -> Result<MigrationInfo> {
    self.with_backend(|backend, migrations, _| {
      let records = backend.list_migrations()?;
      let version = resolve_version(&records, version)?;
      backend.delete(version)?;
      Ok(build_info(&backend.list_migrations()?, migrations))
    })
  }

  /// Drop the metadata table and replay every migration.
  pub fn reset(&mut self) -> Result<MigrationInfo> {
    self.with_backend(|backend, migrations, options| {
      if options.dry_run {
        println!("-- reset metadata and replay {} migrations", migrations.len());
        return Ok(build_info(&[], migrations));
      }
      backend.reset()?;
      for migration in migrations.values() {
        apply_migration(backend, migration, options)?;
      }
      Ok(build_info(&backend.list_migrations()?, migrations))
    })
  }

  /// Successfully applied versions.
  pub fn applied(&mut self) -> Result<Vec<MigrationRecord>> {
    Ok(
      self
        .backend
        .list_migrations()?
        .into_iter()
        .filter(|record| record.success)
        .collect(),
    )
  }

  fn with_backend<T>(
    &mut self,
    action: impl FnOnce(&mut dyn Backend, &MigrationList, &MigratorOptions) -> Result<T>,
  ) -> Result<T> {
    let options = self.options;
    if options.no_lock || options.dry_run {
      action(&mut *self.backend, &self.migrations, &options)
    } else {
      let mut guard = LockGuard::acquire(&mut *self.backend)?;
      action(&mut *guard, &self.migrations, &options)
    }
  }
}

fn build_info(records: &[MigrationRecord], migrations: &MigrationList) -> MigrationInfo {
  let applied = records
    .iter()
    .filter(|record| record.success)
    .map(|record| record.version)
    .collect::<Vec<_>>();
  let applied_set = applied.iter().copied().collect::<BTreeSet<_>>();
  let pending = migrations
    .keys()
    .copied()
    .filter(|version| !applied_set.contains(version))
    .collect();
  let dirty = dirty_version(records);

  MigrationInfo {
    version: 1,
    applied,
    pending,
    dirty,
  }
}

fn dirty_version(records: &[MigrationRecord]) -> Option<i64> {
  records
    .iter()
    .filter(|record| !record.success)
    .map(|record| record.version)
    .min()
}

fn ensure_not_dirty(records: &[MigrationRecord]) -> Result<()> {
  match dirty_version(records) {
    Some(version) => Err(Error::DirtyMigration { version }),
    None => Ok(()),
  }
}

fn validate_migrations(
  records: &[MigrationRecord],
  migrations: &MigrationList,
  mode: &ChecksumMode,
  options: &MigratorOptions,
) -> Result<()> {
  if !options.ignore_missing {
    for record in records.iter().filter(|record| record.success) {
      if !migrations.contains_key(&record.version) {
        return Err(Error::MigrationNotFound {
          version: record.version,
        });
      }
    }
  }
  validate_checksums(records, migrations, mode)
}

fn validate_checksums(
  records: &[MigrationRecord],
  migrations: &MigrationList,
  mode: &ChecksumMode,
) -> Result<()> {
  for record in records.iter().filter(|record| record.success) {
    let Some(migration) = migrations.get(&record.version) else {
      continue;
    };
    if record.is_unverified() {
      continue;
    }
    let expected = mode.checksum(&migration.up);
    if record.checksum.as_slice() != expected {
      return Err(Error::ChecksumMismatch {
        version: record.version,
        expected: checksum_hex(&expected),
        actual: record.checksum_hex(),
      });
    }
    let expected_down = migration.down.as_deref().map(|down| mode.checksum(down));
    if let (Some(stored), Some(expected)) = (&record.down_checksum, expected_down) {
      if !crate::migration::checksum_is_unverified(stored) && stored.as_slice() != expected {
        return Err(Error::ChecksumMismatch {
          version: record.version,
          expected: checksum_hex(&expected),
          actual: checksum_hex(stored),
        });
      }
    }
  }
  Ok(())
}

fn pending_versions(
  records: &[MigrationRecord],
  migrations: &MigrationList,
  target: Option<i64>,
) -> Vec<i64> {
  let applied = records
    .iter()
    .filter(|record| record.success)
    .map(|record| record.version)
    .collect::<BTreeSet<_>>();
  migrations
    .keys()
    .copied()
    .filter(|version| !applied.contains(version))
    .filter(|version| target.is_none_or(|target| *version <= target))
    .collect()
}

fn resolve_version(records: &[MigrationRecord], version: Option<i64>) -> Result<i64> {
  let version = match version {
    Some(version) => version,
    None => dirty_version(records)
      .ok_or_else(|| Error::Migration("there is no dirty migration to resolve".to_string()))?,
  };
  if !records.iter().any(|record| record.version == version) {
    return Err(Error::MigrationNotFound { version });
  }
  Ok(version)
}

fn record_for(
  migration: &Migration,
  execution_time: i64,
  success: bool,
  mode: &ChecksumMode,
) -> MigrationRecord {
  MigrationRecord {
    version: migration.version,
    description: migration.description.clone(),
    installed_on: String::new(),
    success,
    checksum: mode.checksum(&migration.up).to_vec(),
    execution_time,
    down_checksum: migration.down.as_deref().map(|down| mode.checksum(down).to_vec()),
  }
}

fn apply_migration(
  backend: &mut dyn Backend,
  migration: &Migration,
  options: &MigratorOptions,
) -> Result<()> {
  if options.dry_run {
    println!("-- apply {:013} {}", migration.version, migration.description);
    println!("{}", migration.up);
    return Ok(());
  }

  if !options.quiet {
    println!(
      "{} migration: {:013} {}",
      style("Applying").bold().green(),
      migration.version,
      migration.description
    );
  }
  let started = Instant::now();
  let mode = backend.checksum_mode();
  let transactional = options.transaction == TransactionMode::PerMigration
    && !migration.no_tx
    && backend.supports(Feature::TransactionalDdl);

  if transactional {
    backend.begin()?;
    // Re-check under the transaction: on SQLite `BEGIN IMMEDIATE` is the lock,
    // so a concurrent runner may have applied this version while we waited.
    if backend
      .list_migrations()?
      .iter()
      .any(|record| record.version == migration.version && record.success)
    {
      backend.commit()?;
      return Ok(());
    }
    if let Err(error) = backend.execute_batch(&migration.up) {
      let _ = backend.rollback();
      let dirty = record_for(migration, execution_time(started), false, &mode);
      write_metadata(backend, &dirty, true)?;
      return Err(error);
    }
    let record = record_for(migration, execution_time(started), true, &mode);
    backend.upsert(&record)?;
    backend.commit()?;
  } else {
    match backend.execute_batch(&migration.up) {
      Ok(()) => {
        let record = record_for(migration, execution_time(started), true, &mode);
        write_metadata(backend, &record, true)?;
      },
      Err(error) => {
        let dirty = record_for(migration, execution_time(started), false, &mode);
        write_metadata(backend, &dirty, true)?;
        return Err(error);
      },
    }
  }

  Ok(())
}

fn revert_migration(
  backend: &mut dyn Backend,
  migration: &Migration,
  options: &MigratorOptions,
) -> Result<()> {
  let down = migration.down.as_deref().ok_or_else(|| {
    Error::Migration(format!(
      "migration {:013} has no DOWN section and cannot be reverted",
      migration.version
    ))
  })?;

  if options.dry_run {
    println!("-- revert {:013} {}", migration.version, migration.description);
    println!("{down}");
    return Ok(());
  }

  if !options.quiet {
    println!(
      "{} migration: {:013} {}",
      style("Reverting").bold().yellow(),
      migration.version,
      migration.description
    );
  }

  if options.transaction == TransactionMode::PerMigration
    && backend.supports(Feature::TransactionalDdl)
    && !migration.no_tx
  {
    backend.begin()?;
    if let Err(error) = backend.execute_batch(down) {
      let _ = backend.rollback();
      return Err(error);
    }
    backend.delete(migration.version)?;
    backend.commit()?;
  } else {
    backend.execute_batch(down)?;
    backend.delete(migration.version)?;
  }

  Ok(())
}

fn write_metadata(backend: &mut dyn Backend, record: &MigrationRecord, upsert: bool) -> Result<()> {
  backend.begin()?;
  let result = if upsert {
    backend.upsert(record)
  } else {
    backend.insert(record)
  };
  match result {
    Ok(()) => backend.commit(),
    Err(error) => {
      let _ = backend.rollback();
      Err(error)
    },
  }
}

fn execution_time(started: Instant) -> i64 {
  started.elapsed().as_nanos().min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::migration::MigrationFormat;
  use crate::sequel::memory::MemoryBackend;

  fn migration(version: i64, down: bool) -> Migration {
    Migration::from_parts(
      version,
      format!("migration_{version}"),
      format!("create table t{version} (id integer);\n"),
      down.then(|| format!("drop table t{version};\n")),
      false,
      format!("{version:013}_migration_{version}.sql"),
      MigrationFormat::Midas,
    )
  }

  fn migrator(migrations: &[Migration]) -> Migrator {
    let list = migrations
      .iter()
      .cloned()
      .map(|migration| (migration.version, migration))
      .collect();
    Migrator::new(Box::new(MemoryBackend::new()), list)
  }

  #[test]
  fn applies_and_reverts_migrations() {
    let mut migrator = migrator(&[migration(1, true), migration(2, true)]);
    migrator.up(None).expect("up");
    assert_eq!(migrator.info().expect("info").applied, vec![1, 2]);

    migrator.revert_to(Some(1)).expect("revert");
    assert_eq!(migrator.info().expect("info").applied, vec![1]);
  }

  #[test]
  fn up_is_idempotent() {
    let mut migrator = migrator(&[migration(1, true)]);
    migrator.up(None).expect("first up");
    migrator.up(None).expect("second up");
    assert_eq!(migrator.info().expect("info").applied.len(), 1);
  }

  #[test]
  fn target_limits_the_batch() {
    let mut migrator = migrator(&[migration(1, true), migration(2, true), migration(3, true)]);
    migrator.up(Some(2)).expect("up");
    assert_eq!(migrator.info().expect("info").applied, vec![1, 2]);
  }

  #[test]
  fn checksum_mismatch_is_detected() {
    let mut migrator = migrator(&[migration(1, true)]);
    migrator.up(None).expect("up");

    let mut changed = migration(1, true);
    changed.up = "create table changed (id integer);\n".to_string();
    changed.checksum_up = crate::migration::checksum(&changed.up);
    migrator.migrations.insert(1, changed);
    let error = migrator.up(None).expect_err("checksum mismatch");
    assert!(matches!(error, Error::ChecksumMismatch { version: 1, .. }));
  }

  #[test]
  fn dirty_state_blocks_up_and_can_be_resolved() {
    let mut migrator = migrator(&[migration(1, true), migration(2, true)]);
    migrator.up(Some(1)).expect("up");
    let record = MigrationRecord::applied(1, "migration_1", vec![0; 48], None, 0);
    migrator.backend.upsert(&record).expect("dirty");

    // Replace with a genuinely failed row.
    let mut failed = record;
    failed.success = false;
    migrator.backend.upsert(&failed).expect("dirty");
    let error = migrator.up(None).expect_err("dirty blocks");
    assert!(matches!(error, Error::DirtyMigration { version: 1 }));

    migrator.resolve_applied(None).expect("resolve");
    migrator.up(None).expect("up after resolve");
  }

  #[test]
  fn baseline_does_not_execute_sql() {
    let mut migrator = migrator(&[migration(1, true), migration(2, true)]);
    migrator.baseline(2).expect("baseline");
    assert_eq!(migrator.info().expect("info").applied, vec![1, 2]);
  }

  #[test]
  fn dry_run_leaves_state_untouched() {
    let mut migrator = migrator(&[migration(1, true)]).with_options(MigratorOptions {
      dry_run: true,
      ..MigratorOptions::default()
    });
    migrator.up(None).expect("dry run");
    assert!(migrator.info().expect("info").applied.is_empty());
  }
}
