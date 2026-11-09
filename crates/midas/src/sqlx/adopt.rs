//! SQLx metadata adoption and baselining.

use std::collections::{
  BTreeMap,
  BTreeSet,
};

use crate::commander::MigratorOptions;
use crate::config::Config;
use crate::migration::{
  discover,
  LockGuard,
  Migration,
  MigrationRecord,
};
use crate::sequel::{
  self,
  Backend,
  ChecksumMode,
};
use crate::sqlx::config::split_qualified_table;
use crate::{
  Error,
  Result,
};

const DEFAULT_SQLX_TABLE: &str = "_sqlx_migrations";
const SQLX_COLUMNS: &[&str] = &[
  "version",
  "description",
  "installed_on",
  "success",
  "checksum",
  "execution_time",
];

/// Result of inspecting an existing SQLx metadata table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptionReport {
  /// Schema the table lives in, when one was configured.
  pub schema: Option<String>,
  /// Table name.
  pub table: String,
  /// Successfully applied versions.
  pub applied: Vec<i64>,
  /// Whether the table stores DOWN checksums.
  pub has_down_checksum: bool,
}

/// Result of importing SQLx metadata into the Midas metadata table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BaselineReport {
  /// Versions inserted into the Midas metadata table.
  pub inserted: Vec<i64>,
  /// Versions already present and therefore left untouched.
  pub skipped: Vec<i64>,
}

/// Build a configuration that targets a specific metadata table.
pub fn config_for_table(config: &Config, table: &str) -> Result<Config> {
  let (schema, table) = split_qualified_table(table)?;
  let mut config = config.clone();
  config.metadata_schema = schema;
  config.metadata_table = table;
  Ok(config)
}

/// Verify that a SQLx-controlled metadata table can be adopted without
/// modifying it.
pub fn verify_adoption(config: &Config, table: &str) -> Result<AdoptionReport> {
  let config = config_for_table(config, table)?;
  let mut backend = sequel::connect_existing(&config)?;
  adoption_report(&mut *backend, &config)
}

/// Detect the SQLx metadata table configured by `sqlx.toml`, or the SQLx
/// default when no table is configured.
pub fn detect(config: &Config) -> Result<Option<AdoptionReport>> {
  detect_table(config, &sqlx_table_name(config))
}

/// Inspect an explicit SQLx metadata table.
pub fn detect_table(config: &Config, table: &str) -> Result<Option<AdoptionReport>> {
  let source = config_for_table(config, table)?;
  let mut backend = sequel::connect_existing(&source)?;
  let info = backend.metadata_info()?;
  if !info.exists {
    return Ok(None);
  }
  validate_sqlx_columns(&info.columns, table)?;
  Ok(Some(report_from_records(
    &source,
    &mut *backend,
    info.has_down_checksum(),
  )?))
}

/// Copy applied SQLx metadata rows into the Midas metadata table.
///
/// SQL is never executed. Applied versions must exist on disk and, unless the
/// stored checksum is a legacy placeholder, checksums must match the local
/// migration using SQLx's checksum rules.
pub fn baseline_from_sqlx(config: &Config, options: &MigratorOptions) -> Result<BaselineReport> {
  let table = sqlx_table_name(config);
  let source_config = config_for_table(config, &table)?;
  let mut source = sequel::connect_existing(&source_config)?;
  let source_info = source.metadata_info()?;
  if !source_info.exists {
    return Err(Error::migration(format!(
      "SQLx metadata table `{table}` does not exist"
    )));
  }
  validate_sqlx_columns(&source_info.columns, &table)?;

  let records = source.list_migrations()?;
  ensure_success(&records)?;
  let migrations = discover(&config.migrations_dir)?;
  let mode = ChecksumMode::Sqlx {
    ignored_chars: config.sqlx.ignored_chars.clone(),
  };

  for record in &records {
    let migration = migrations.get(&record.version).ok_or(Error::MigrationNotFound {
      version: record.version,
    })?;
    if !record.is_unverified() {
      let expected = mode.checksum(&migration.up);
      if record.checksum.as_slice() != expected {
        return Err(Error::ChecksumMismatch {
          version: record.version,
          expected: crate::migration::checksum_hex(&expected),
          actual: record.checksum_hex(),
        });
      }
    }
  }

  let mut target = sequel::connect(config)?;
  if options.dry_run || options.no_lock {
    return baseline_records(
      &mut *target,
      &records,
      &migrations,
      &source_config,
      config,
      options,
    );
  }

  let mut guard = LockGuard::acquire(&mut *target)?;
  baseline_records(
    &mut *guard,
    &records,
    &migrations,
    &source_config,
    config,
    options,
  )
}

fn sqlx_table_name(config: &Config) -> String {
  config
    .sqlx
    .table_name
    .clone()
    .unwrap_or_else(|| DEFAULT_SQLX_TABLE.to_string())
}

fn adoption_report(backend: &mut dyn Backend, config: &Config) -> Result<AdoptionReport> {
  let info = backend.metadata_info()?;
  if !info.exists {
    return Err(Error::migration(format!(
      "SQLx metadata table `{}` does not exist",
      config.metadata_table
    )));
  }
  validate_sqlx_columns(&info.columns, &config.metadata_table)?;
  report_from_records(config, backend, info.has_down_checksum())
}

fn report_from_records(
  config: &Config,
  backend: &mut dyn Backend,
  has_down_checksum: bool,
) -> Result<AdoptionReport> {
  let records = backend.list_migrations()?;
  ensure_success(&records)?;
  Ok(AdoptionReport {
    schema: config.metadata_schema.clone(),
    table: config.metadata_table.clone(),
    applied: records
      .iter()
      .filter(|record| record.success)
      .map(|record| record.version)
      .collect(),
    has_down_checksum,
  })
}

fn validate_sqlx_columns(columns: &BTreeSet<String>, table: &str) -> Result<()> {
  let missing = SQLX_COLUMNS
    .iter()
    .filter(|column| !columns.contains(**column))
    .copied()
    .collect::<Vec<_>>();
  if missing.is_empty() {
    return Ok(());
  }
  Err(Error::migration(format!(
    "`{table}` is not a SQLx metadata table; missing columns: {}",
    missing.join(", ")
  )))
}

fn ensure_success(records: &[MigrationRecord]) -> Result<()> {
  match records
    .iter()
    .filter(|record| !record.success)
    .map(|record| record.version)
    .min()
  {
    Some(version) => Err(Error::DirtyMigration { version }),
    None => Ok(()),
  }
}

fn baseline_records(
  backend: &mut dyn Backend,
  records: &[MigrationRecord],
  migrations: &BTreeMap<i64, Migration>,
  source_config: &Config,
  config: &Config,
  options: &MigratorOptions,
) -> Result<BaselineReport> {
  let same_table = config.metadata_table == source_config.metadata_table
    && config.metadata_schema == source_config.metadata_schema;
  if same_table {
    // Already pointing at the SQLx table; validation above is the whole job.
    return Ok(BaselineReport {
      inserted: Vec::new(),
      skipped: records.iter().map(|record| record.version).collect(),
    });
  }

  let existing = backend
    .list_migrations()?
    .into_iter()
    .filter(|record| record.success)
    .map(|record| record.version)
    .collect::<BTreeSet<_>>();
  let target_mode = backend.checksum_mode();
  let mut report = BaselineReport::default();

  if options.dry_run {
    for record in records {
      let migration = migrations.get(&record.version).ok_or(Error::MigrationNotFound {
        version: record.version,
      })?;
      if existing.contains(&record.version) {
        report.skipped.push(record.version);
        continue;
      }
      println!("-- baseline {:013} {}", migration.version, migration.description);
      report.inserted.push(record.version);
    }
    return Ok(report);
  }

  backend.begin()?;
  for record in records {
    if existing.contains(&record.version) {
      report.skipped.push(record.version);
      continue;
    }

    let Some(migration) = migrations.get(&record.version) else {
      let _ = backend.rollback();
      return Err(Error::MigrationNotFound {
        version: record.version,
      });
    };

    // Recomputing is cheap and keeps the stored checksum consistent with the
    // configured target metadata table rather than the SQLx source table.
    let checksum = target_mode.checksum(&migration.up).to_vec();
    let down_checksum = migration
      .down
      .as_deref()
      .map(|down| target_mode.checksum(down).to_vec());
    let inserted = MigrationRecord::applied(
      migration.version,
      migration.description.clone(),
      checksum,
      down_checksum,
      record.execution_time,
    );
    if let Err(error) = backend.insert(&inserted) {
      let _ = backend.rollback();
      return Err(error);
    }
    report.inserted.push(record.version);
  }
  backend.commit()?;

  Ok(report)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn table_configuration_is_parsed() {
    let config = Config::default();
    let scoped = config_for_table(&config, "foo._sqlx_migrations").expect("parse");
    assert_eq!(scoped.metadata_schema.as_deref(), Some("foo"));
    assert_eq!(scoped.metadata_table, "_sqlx_migrations");
  }

  #[test]
  fn missing_columns_are_reported() {
    let columns = ["version", "description"]
      .into_iter()
      .map(ToString::to_string)
      .collect();
    let error = validate_sqlx_columns(&columns, "_sqlx_migrations").expect_err("missing columns");
    assert!(error.to_string().contains("missing columns"));
  }

  #[test]
  fn dirty_rows_are_rejected() {
    let mut record = MigrationRecord::applied(1, "init", vec![1; 48], None, 1);
    record.success = false;
    let error = ensure_success(&[record]).expect_err("dirty");
    assert!(matches!(error, Error::DirtyMigration { version: 1 }));
  }
}
