//! Migration runner.

use std::collections::BTreeSet;
use std::time::{
  Duration,
  Instant,
};

use console::style;
use indicatif::ProgressBar;
use prettytable::format::consts;
use prettytable::{
  color,
  row,
  Attr,
  Cell,
  Row,
  Table,
};
use url::Url;

use crate::lookup::{
  MigrationFile,
  MigrationFiles,
};
use crate::sequel::{
  AppliedMigration,
  Backend,
  DatabaseEngine,
};
use crate::{
  progress_style,
  Error,
  Result,
};

/// Coordinates migration files with a database backend.
pub struct Migrator {
  backend: Box<dyn Backend>,
  migrations: MigrationFiles,
}

impl Migrator {
  /// Create a migrator from a backend and migration set.
  pub fn new(backend: Box<dyn Backend>, migrations: MigrationFiles) -> Self {
    Self { backend, migrations }
  }

  /// Print the current migration status.
  pub fn status(&mut self) -> Result<()> {
    let applied = self.backend.list_applied()?;
    let applied_versions = applied
      .iter()
      .map(|migration| migration.version)
      .collect::<BTreeSet<_>>();
    let available = self.migrations.keys().copied().collect::<Vec<_>>();

    if available.is_empty() {
      println!("There are no available migration files.");
      return Ok(());
    }

    let mut table = Table::new();
    table.set_titles(row![Fbb->"Migration No.", Fbb->"Status", Fbb->"Filename"]);
    table.set_format(*consts::FORMAT_CLEAN);

    for version in &available {
      let active = applied_versions.contains(version);
      let temp_color = if active { color::GREEN } else { color::RED };
      let Some(migration) = self.migrations.get(version) else {
        continue;
      };

      table.add_row(Row::new(vec![
        Cell::new(&format!("{version:013}")).with_style(Attr::Bold),
        Cell::new(if active { "Active" } else { "Inactive" }).with_style(Attr::ForegroundColor(temp_color)),
        Cell::new(&migration.filename).with_style(Attr::ForegroundColor(temp_color)),
      ]));
    }

    let msg = style("Available migrations:").bold().cyan();
    println!();
    println!("{msg}");
    println!();
    table.printstd();
    println!();

    let completed_migrations = style("Completed migrations:").bold().cyan();
    let total_migrations = style("Total migrations:").bold().cyan();
    println!("{completed_migrations}: {}", applied.len());
    println!("{total_migrations}: {}", available.len());

    Ok(())
  }

  /// Apply every pending migration.
  pub fn up(&mut self) -> Result<()> {
    if self.migrations.is_empty() {
      println!("There are no available migration files.");
      return Ok(());
    }

    let applied = self.applied_versions()?;
    let pending = self
      .migrations
      .keys()
      .copied()
      .filter(|version| !applied.contains(version))
      .collect::<Vec<_>>();
    self.apply_all(&pending, "Applying")
  }

  /// Apply pending migrations up to and including `migration_number`.
  pub fn upto(&mut self, migration_number: i64) -> Result<()> {
    if !self.migrations.contains_key(&migration_number) {
      return Err(Error::MigrationNotFound {
        version: migration_number,
      });
    }

    let applied = self.applied_versions()?;
    let pending = self
      .migrations
      .keys()
      .copied()
      .filter(|version| !applied.contains(version) && *version <= migration_number)
      .collect::<Vec<_>>();
    self.apply_all(&pending, "Applying")
  }

  /// Revert every applied migration.
  pub fn down(&mut self) -> Result<()> {
    let applied = self.backend.list_applied()?;
    if applied.is_empty() {
      println!("Migrations table is empty. No need to run down migrations.");
      return Ok(());
    }

    let pb = self.progress_bar(applied.len() as u64, None)?;
    for record in applied.iter().rev() {
      let version = record.version;
      let migration = self
        .migrations
        .get(&version)
        .ok_or(Error::MigrationNotFound { version })?;
      pb.set_prefix(format!("{version:013}"));
      pb.set_message(format!("Undoing migration: {}", migration.description()));

      self.backend.execute_batch(&migration.down_sql())?;
      self.backend.delete(version)?;
      pb.inc(1);
    }
    pb.finish();

    Ok(())
  }

  /// Re-apply the last applied migration.
  pub fn redo(&mut self) -> Result<()> {
    let Some(last) = self.backend.get_last()? else {
      println!("Migrations table is empty. No need to redo migrations.");
      return Ok(());
    };

    let version = last.version;
    let migration = self
      .migrations
      .get(&version)
      .ok_or(Error::MigrationNotFound { version })?;
    let pb = self.progress_bar(1, Some(version))?;

    pb.set_message(format!("Undoing migration: {}", migration.description()));
    self.backend.execute_batch(&migration.down_sql())?;
    self.backend.delete(version)?;

    pb.set_message(format!("Applying migration: {}", migration.description()));
    let start = Instant::now();
    self.backend.execute_batch(&migration.up_sql())?;
    record_applied(&mut *self.backend, migration, version, start)?;
    pb.inc(1);
    pb.finish();

    Ok(())
  }

  /// Revert the last applied migration.
  pub fn revert(&mut self) -> Result<()> {
    let Some(last) = self.backend.get_last()? else {
      println!("Migrations table is empty. No need to run revert migrations.");
      return Ok(());
    };

    let version = last.version;
    let migration = self
      .migrations
      .get(&version)
      .ok_or(Error::MigrationNotFound { version })?;
    let pb = self.progress_bar(1, Some(version))?;

    pb.set_message(format!("Reverting migration: {}", migration.description()));
    self.backend.execute_batch(&migration.down_sql())?;
    self.backend.delete(version)?;
    pb.inc(1);
    pb.finish();

    Ok(())
  }

  /// Drop and recreate the database referenced by `db_url`.
  pub fn drop(&mut self, db_url: &str) -> Result<()> {
    let database_name = database_name(db_url)?;
    self.backend.force_drop_database(&database_name)?;
    self.backend.create_database(&database_name)?;
    Ok(())
  }

  /// Applied migrations, ordered by version.
  pub fn applied(&mut self) -> Result<Vec<AppliedMigration>> {
    self.backend.list_applied()
  }

  fn applied_versions(&mut self) -> Result<BTreeSet<i64>> {
    Ok(
      self
        .backend
        .list_applied()?
        .into_iter()
        .map(|migration| migration.version)
        .collect(),
    )
  }

  fn apply_all(&mut self, versions: &[i64], action: &str) -> Result<()> {
    if versions.is_empty() {
      println!("Migrations are all up-to-date.");
      return Ok(());
    }

    let pb = self.progress_bar(versions.len() as u64, None)?;
    for version in versions {
      let migration = self
        .migrations
        .get(version)
        .ok_or(Error::MigrationNotFound { version: *version })?;
      pb.set_prefix(format!("{version:013}"));
      pb.set_message(format!("{action} migration: {}", migration.description()));

      let start = Instant::now();
      self.backend.execute_batch(&migration.up_sql())?;
      record_applied(&mut *self.backend, migration, *version, start)?;
      pb.inc(1);
    }
    pb.finish();

    Ok(())
  }

  fn progress_bar(&self, len: u64, prefix: Option<i64>) -> Result<ProgressBar> {
    let pb = ProgressBar::new(len);
    pb.set_style(progress_style()?);
    pb.enable_steady_tick(Duration::from_millis(80));
    if let Some(version) = prefix {
      pb.set_prefix(format!("{version:013}"));
    }
    Ok(pb)
  }
}

fn record_applied(
  backend: &mut dyn Backend,
  migration: &MigrationFile,
  version: i64,
  start: Instant,
) -> Result<()> {
  let execution_time_ms = start.elapsed().as_millis().min(i64::MAX as u128) as i64;
  backend.insert(&AppliedMigration {
    version,
    description: migration.description().to_string(),
    checksum: String::new(),
    execution_time_ms,
    success: true,
  })
}

fn database_name(db_url: &str) -> Result<String> {
  if DatabaseEngine::from_url(db_url)? == DatabaseEngine::Sqlite {
    return Ok("sqlite".to_string());
  }

  let url = Url::parse(db_url).map_err(|error| Error::config(format!("invalid database URL: {error}")))?;
  url
    .path_segments()
    .and_then(|mut segments| segments.next_back())
    .filter(|name| !name.is_empty())
    .map(ToString::to_string)
    .ok_or_else(|| Error::config("database name not found in URL"))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::sequel::memory::MemoryBackend;

  fn migration(version: i64) -> MigrationFile {
    MigrationFile {
      content_up: Some(vec!["create table t (id integer);".to_string()]),
      content_down: Some(vec!["drop table t;".to_string()]),
      number: version,
      filename: format!("{version:013}_create_table.sql"),
    }
  }

  fn migrator() -> Migrator {
    let mut migrations = MigrationFiles::new();
    migrations.insert(1, migration(1));
    Migrator::new(Box::new(MemoryBackend::new()), migrations)
  }

  #[test]
  fn applies_migration_against_memory_store() {
    let mut migrator = migrator();
    migrator.up().expect("up");

    let applied = migrator.applied().expect("applied");
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].version, 1);
    assert!(applied[0].success);
  }

  #[test]
  fn up_is_idempotent() {
    let mut migrator = migrator();
    migrator.up().expect("first up");
    migrator.up().expect("second up");
    assert_eq!(migrator.applied().expect("applied").len(), 1);
  }

  #[test]
  fn revert_removes_last_migration() {
    let mut migrator = migrator();
    migrator.up().expect("up");
    migrator.revert().expect("revert");
    assert!(migrator.applied().expect("applied").is_empty());
  }

  #[test]
  fn down_removes_every_migration() {
    let mut migrations = MigrationFiles::new();
    migrations.insert(1, migration(1));
    migrations.insert(2, migration(2));
    let mut migrator = Migrator::new(Box::new(MemoryBackend::new()), migrations);

    migrator.up().expect("up");
    migrator.down().expect("down");
    assert!(migrator.applied().expect("applied").is_empty());
  }

  #[test]
  fn upto_rejects_unknown_version() {
    let mut migrator = migrator();
    let error = migrator.upto(99).expect_err("unknown version");
    assert!(matches!(error, Error::MigrationNotFound { version: 99 }));
  }

  #[test]
  fn database_name_is_extracted() {
    assert_eq!(
      database_name("postgres://user@localhost:5432/app").expect("name"),
      "app"
    );
    assert_eq!(database_name("./data.db3").expect("name"), "sqlite");
    assert_eq!(database_name("file://./data.db3").expect("name"), "sqlite");
  }
}
