//! Phase 2 SQLx adoption and baseline tests against SQLite.

#![cfg(feature = "sqlite")]

use std::fs;
use std::path::Path;

use midas::commander::{
  Migrator,
  MigratorOptions,
};
use midas::config::Config;
use midas::migration::{
  checksum_hex,
  discover,
  ChecksumMode,
  MigrationRecord,
};
use midas::sequel::sqlite::Sqlite;
use midas::sequel::{
  self,
  Executor,
};
use midas::sqlx::{
  baseline_from_sqlx,
  verify_adoption,
};
use midas::Error;

fn sqlx_checksum(sql: &str) -> Vec<u8> {
  ChecksumMode::Sqlx {
    ignored_chars: Default::default(),
  }
  .checksum(sql)
  .to_vec()
}

fn create_sqlx_table(path: &Path, rows: &[(i64, &str, bool, Vec<u8>, i64)]) {
  let mut backend = Sqlite::new(&path.to_string_lossy()).expect("open sqlite");
  let mut sql = String::from(
    "drop table if exists _sqlx_migrations;
     create table _sqlx_migrations (
       version integer not null primary key,
       description text not null,
       installed_on text not null default (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
       success boolean not null,
       checksum blob not null,
       execution_time integer not null
     );
     ",
  );
  for (version, description, success, checksum, execution_time) in rows {
    sql.push_str(&format!(
      "insert into _sqlx_migrations \
       (version, description, success, checksum, execution_time) \
       values ({version}, '{description}', {}, X'{}', {execution_time});\n",
      i32::from(*success),
      checksum_hex(checksum),
    ));
  }
  backend.execute_batch(&sql).expect("create sqlx table");
}

fn write_migration(dir: &Path, name: &str, up: &str, down: &str) {
  fs::create_dir_all(dir).expect("create migrations");
  fs::write(dir.join(name), format!("-- !UP\n{up}\n-- !DOWN\n{down}\n")).expect("write migration");
}

fn adoption_config(path: &Path) -> Config {
  Config {
    database_url: Some(path.to_string_lossy().into_owned()),
    metadata_schema: None,
    metadata_table: "_sqlx_migrations".to_string(),
    ..Config::default()
  }
}

fn midas_config(path: &Path, migrations: &Path) -> Config {
  Config {
    database_url: Some(path.to_string_lossy().into_owned()),
    migrations_dir: migrations.to_path_buf(),
    ..Config::default()
  }
}

#[test]
fn adopted_sqlx_table_is_never_modified_and_sql_does_not_rerun() {
  let temp = tempfile::tempdir().expect("temp dir");
  let db = temp.path().join("app.db3");
  let migrations = temp.path().join("migrations");
  let up = "create table widget (id integer);\n";
  write_migration(&migrations, "1_init.sql", up, "drop table widget;");
  let actual_up = discover(&migrations).expect("discover")[&1].up.clone();
  create_sqlx_table(&db, &[(1, "init", true, sqlx_checksum(&actual_up), 7)]);

  let config = adoption_config(&db);
  let mut backend = sequel::connect(&config).expect("adopt");
  let info = backend.metadata_info().expect("metadata info");
  assert!(info.sqlx_managed);
  assert!(!info.has_down_checksum(), "adoption must not add columns");

  let mut migrator = Migrator::new(
    sequel::connect(&config).expect("connect"),
    discover(&migrations).expect("discover"),
  );
  let state = migrator.up(None).expect("up is a no-op");
  assert_eq!(state.applied, vec![1]);

  let mut verify = sequel::connect_existing(&config).expect("reconnect");
  let info = verify.metadata_info().expect("metadata info");
  assert!(!info.has_down_checksum());
  assert!(info.columns.contains("version"));
}

#[test]
fn dirty_sqlx_rows_are_rejected() {
  let temp = tempfile::tempdir().expect("temp dir");
  let db = temp.path().join("app.db3");
  let migrations = temp.path().join("migrations");
  let up = "select 1;\n";
  write_migration(&migrations, "1_init.sql", up, "select 1;");
  let actual_up = discover(&migrations).expect("discover")[&1].up.clone();
  create_sqlx_table(&db, &[(1, "init", false, sqlx_checksum(&actual_up), 1)]);

  let config = adoption_config(&db);
  let error = verify_adoption(&config, "_sqlx_migrations").expect_err("dirty rows fail");
  assert!(matches!(error, Error::DirtyMigration { version: 1 }));
}

#[test]
fn baseline_copies_sqlx_records_without_running_sql() {
  let temp = tempfile::tempdir().expect("temp dir");
  let db = temp.path().join("app.db3");
  let migrations = temp.path().join("migrations");
  let up = "this is not valid sql;\n";
  write_migration(&migrations, "1_init.sql", up, "select 1;");
  let actual_up = discover(&migrations).expect("discover")[&1].up.clone();
  create_sqlx_table(&db, &[(1, "init", true, sqlx_checksum(&actual_up), 123_456_789)]);

  let config = midas_config(&db, &migrations);
  let report = baseline_from_sqlx(&config, &MigratorOptions::default()).expect("baseline");
  assert_eq!(report.inserted, vec![1]);
  assert!(report.skipped.is_empty());

  let mut backend = sequel::connect_existing(&config).expect("connect target");
  let records = backend.list_migrations().expect("list");
  assert_eq!(records.len(), 1);
  assert_eq!(records[0].version, 1);
  assert_eq!(records[0].execution_time, 123_456_789);
  assert!(records[0].success);
  assert_eq!(
    records[0].checksum,
    ChecksumMode::Midas.checksum(&actual_up).to_vec()
  );
}

#[test]
fn baseline_rejects_modified_migrations() {
  let temp = tempfile::tempdir().expect("temp dir");
  let db = temp.path().join("app.db3");
  let migrations = temp.path().join("migrations");
  write_migration(
    &migrations,
    "1_init.sql",
    "create table widget (id integer);\n",
    "drop table widget;",
  );
  create_sqlx_table(&db, &[(1, "init", true, vec![7; 48], 1)]);

  let config = midas_config(&db, &migrations);
  let error = baseline_from_sqlx(&config, &MigratorOptions::default()).expect_err("mismatch");
  assert!(matches!(error, Error::ChecksumMismatch { version: 1, .. }));
}

#[test]
fn baseline_requires_local_files_for_applied_versions() {
  let temp = tempfile::tempdir().expect("temp dir");
  let db = temp.path().join("app.db3");
  let migrations = temp.path().join("migrations");
  fs::create_dir_all(&migrations).expect("create migrations");
  create_sqlx_table(&db, &[(1, "init", true, sqlx_checksum("select 1;"), 1)]);

  let config = midas_config(&db, &migrations);
  let error = baseline_from_sqlx(&config, &MigratorOptions::default()).expect_err("missing file");
  assert!(matches!(error, Error::MigrationNotFound { version: 1 }));
}

#[test]
fn missing_local_migration_requires_ignore_missing() {
  let temp = tempfile::tempdir().expect("temp dir");
  let db = temp.path().join("app.db3");
  let migrations = temp.path().join("migrations");
  let path = migrations.join("1_init.sql");
  write_migration(&migrations, "1_init.sql", "select 1;\n", "select 1;");

  let config = midas_config(&db, &migrations);
  let mut migrator = Migrator::new(
    sequel::connect(&config).expect("connect"),
    discover(&migrations).expect("discover"),
  );
  migrator.up(None).expect("apply");

  fs::remove_file(&path).expect("remove migration");
  let backend = sequel::connect(&config).expect("connect");
  let mut strict = Migrator::new(backend, discover(&migrations).expect("discover"));
  let error = strict.up(None).expect_err("missing migration is an error");
  assert!(matches!(error, Error::MigrationNotFound { version: 1 }));

  let backend = sequel::connect(&config).expect("connect");
  let mut lenient =
    Migrator::new(backend, discover(&migrations).expect("discover")).with_options(MigratorOptions {
      ignore_missing: true,
      ..MigratorOptions::default()
    });
  lenient.up(None).expect("ignore missing");
}

#[test]
fn sqlx_toml_is_loaded_by_the_configuration_layer() {
  let temp = tempfile::tempdir().expect("temp dir");
  fs::write(
    temp.path().join("sqlx.toml"),
    r#"
[migrate]
table-name = "audit._sqlx_migrations"
migrations-dir = "db/migrations"
create-schemas = ["audit"]
ignored-chars = ["\r"]

[migrate.defaults]
migration-type = "reversible"
migration-versioning = "sequential"
"#,
  )
  .expect("write sqlx.toml");

  let config = Config::load(
    &std::collections::BTreeMap::new(),
    &Default::default(),
    Some(temp.path()),
  )
  .expect("load config");
  assert_eq!(config.metadata_schema.as_deref(), Some("audit"));
  assert_eq!(config.metadata_table, "_sqlx_migrations");
  assert_eq!(config.migrations_dir, Path::new("db/migrations"));
  assert!(config.create_schemas.contains("audit"));
  assert!(config.sqlx.ignored_chars.contains(&'\r'));
}

#[test]
fn ignored_chars_are_used_for_adopted_checksums() {
  let temp = tempfile::tempdir().expect("temp dir");
  let db = temp.path().join("app.db3");
  let migrations = temp.path().join("migrations");
  fs::create_dir_all(&migrations).expect("create migrations");
  fs::write(
    migrations.join("1_init.sql"),
    "-- !UP\r\nselect 1;\r\n-- !DOWN\r\nselect 1;\r\n",
  )
  .expect("write migration");

  let up = discover(&migrations).expect("discover")[&1].up.clone();
  let mut ignored_chars = std::collections::BTreeSet::new();
  ignored_chars.insert('\r');
  let checksum = ChecksumMode::Sqlx { ignored_chars }.checksum(&up);
  create_sqlx_table(&db, &[(1, "init", true, checksum.to_vec(), 1)]);

  let mut config = adoption_config(&db);
  config.sqlx.ignored_chars.insert('\r');
  let mut migrator = Migrator::new(
    sequel::connect(&config).expect("connect"),
    discover(&migrations).expect("discover"),
  );
  migrator
    .up(None)
    .expect("ignored CR does not change the checksum");
}

#[test]
fn record_checksums_use_the_sqlx_contract_for_adopted_tables() {
  let temp = tempfile::tempdir().expect("temp dir");
  let db = temp.path().join("app.db3");
  let migrations = temp.path().join("migrations");
  let up = "select 1;\n";
  write_migration(&migrations, "1_init.sql", up, "select 1;");
  let actual_up = discover(&migrations).expect("discover")[&1].up.clone();
  create_sqlx_table(&db, &[(1, "init", true, sqlx_checksum(&actual_up), 1)]);

  let config = adoption_config(&db);
  let mut backend = sequel::connect(&config).expect("connect");
  let records = backend.list_migrations().expect("list");
  assert_eq!(records[0].checksum, sqlx_checksum(&actual_up));
  let record: MigrationRecord = records[0].clone();
  assert_eq!(record.checksum_hex().len(), 96);
}
