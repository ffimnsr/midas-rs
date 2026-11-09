//! Phase 2 SQLx interop CLI tests.

#![cfg(feature = "sqlite")]

mod common;

use std::fs;
use std::path::Path;

use midas::migration::{
  checksum_hex,
  discover,
  ChecksumMode,
};
use midas::sequel::sqlite::Sqlite;
use midas::sequel::Executor;
use predicates::prelude::*;

use assert_fs::prelude::PathChild as _;

fn write_migration(dir: &Path, name: &str, up: &str, down: &str) {
  fs::create_dir_all(dir).expect("create migrations");
  fs::write(dir.join(name), format!("-- !UP\n{up}\n-- !DOWN\n{down}\n")).expect("write migration");
}

fn database(temp: &assert_fs::TempDir) -> String {
  temp.child("app.db3").path().to_string_lossy().into_owned()
}

fn create_sqlx_table(path: &Path, migrations: &Path) {
  let up = discover(migrations).expect("discover")[&1].up.clone();
  let checksum = ChecksumMode::Sqlx {
    ignored_chars: Default::default(),
  }
  .checksum(&up);

  let mut backend = Sqlite::new(&path.to_string_lossy()).expect("open sqlite");
  backend
    .execute_batch(&format!(
      "drop table if exists _sqlx_migrations;
       create table _sqlx_migrations (
         version integer not null primary key,
         description text not null,
         installed_on text not null default (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
         success boolean not null,
         checksum blob not null,
         execution_time integer not null
       );
       insert into _sqlx_migrations
         (version, description, success, checksum, execution_time)
       values (1, 'init', 1, X'{}', 42);",
      checksum_hex(&checksum)
    ))
    .expect("create sqlx table");
}

#[test]
fn export_import_round_trip_through_both_command_groups() {
  let temp = assert_fs::TempDir::new().expect("temp dir");
  write_migration(
    &temp.path().join("migrations"),
    "1_init.sql",
    "create table widget (id integer);",
    "drop table widget;",
  );

  common::run_midas(
    &[
      "--source",
      "migrations",
      "migrate",
      "export",
      "--format",
      "sqlx",
      "--out",
      "exported",
    ],
    temp.path(),
  )
  .success();

  let exported = temp.path().join("exported");
  assert!(exported.join("0000000000001_init.up.sql").is_file());
  assert!(exported.join("0000000000001_init.down.sql").is_file());

  common::run_midas(
    &["sqlx", "import", "--from", "exported", "--out", "imported"],
    temp.path(),
  )
  .success();

  let imported =
    fs::read_to_string(temp.path().join("imported").join("0000000000001_init.sql")).expect("read imported");
  assert!(imported.contains("-- !UP"));
  assert!(imported.contains("-- !DOWN"));
  assert!(imported.contains("create table widget (id integer);"));
}

#[test]
fn doctor_detects_and_adopt_verifies_a_sqlx_table() {
  let temp = assert_fs::TempDir::new().expect("temp dir");
  let db = database(&temp);
  let migrations = temp.path().join("migrations");
  write_migration(
    &migrations,
    "1_init.sql",
    "create table widget (id integer);",
    "drop table widget;",
  );
  create_sqlx_table(Path::new(&db), &migrations);

  common::run_midas(&["--database", &db, "doctor", "--sqlx"], temp.path())
    .success()
    .stdout(predicate::str::contains("Found SQLx metadata table"))
    .stdout(predicate::str::contains("midas migrate adopt"));

  common::run_midas(
    &[
      "--database",
      &db,
      "migrate",
      "adopt",
      "--table",
      "_sqlx_migrations",
    ],
    temp.path(),
  )
  .success()
  .stdout(predicate::str::contains("adoptable"));
}

#[test]
fn doctor_reports_when_no_sqlx_table_exists() {
  let temp = assert_fs::TempDir::new().expect("temp dir");
  common::run_midas(&["--database", &database(&temp), "doctor", "--sqlx"], temp.path())
    .success()
    .stdout(predicate::str::contains("No SQLx metadata table found"));
}

#[test]
fn baseline_from_sqlx_populates_midas_metadata() {
  let temp = assert_fs::TempDir::new().expect("temp dir");
  let db = database(&temp);
  let migrations = temp.path().join("migrations");
  write_migration(
    &migrations,
    "1_init.sql",
    "create table widget (id integer);",
    "drop table widget;",
  );
  create_sqlx_table(Path::new(&db), &migrations);

  common::run_midas(
    &[
      "--database",
      &db,
      "--source",
      "migrations",
      "migrate",
      "baseline",
      "--from",
      "sqlx",
    ],
    temp.path(),
  )
  .success()
  .stdout(predicate::str::contains("Baselined 1 migration(s)"));

  common::run_midas(
    &["--database", &db, "--source", "migrations", "info", "--json"],
    temp.path(),
  )
  .success()
  .stdout(predicate::str::contains(r#""applied":[1]"#));
}

#[test]
fn ignore_missing_is_opt_in() {
  let temp = assert_fs::TempDir::new().expect("temp dir");
  let db = database(&temp);
  let migrations = temp.path().join("migrations");
  let path = migrations.join("1_init.sql");
  write_migration(
    &migrations,
    "1_init.sql",
    "create table widget (id integer);",
    "drop table widget;",
  );

  common::run_midas(&["--database", &db, "--source", "migrations", "run"], temp.path()).success();

  fs::remove_file(path).expect("remove migration");
  common::run_midas(&["--database", &db, "--source", "migrations", "run"], temp.path()).failure();

  common::run_midas(
    &[
      "--database",
      &db,
      "--source",
      "migrations",
      "--ignore-missing",
      "run",
    ],
    temp.path(),
  )
  .success();
}

#[test]
fn db_create_from_migrations_does_not_create_sqlx_metadata() {
  let temp = assert_fs::TempDir::new().expect("temp dir");
  let db = database(&temp);
  write_migration(
    &temp.path().join("migrations"),
    "1_init.sql",
    "create table widget (id integer);",
    "drop table widget;",
  );

  common::run_midas(
    &[
      "--database",
      &db,
      "db",
      "create",
      "--from-migrations",
      "migrations",
    ],
    temp.path(),
  )
  .success();

  common::run_midas(&["--database", &db, "doctor", "--sqlx"], temp.path())
    .success()
    .stdout(predicate::str::contains("No SQLx metadata table found"));

  common::run_midas(
    &["--database", &db, "--source", "migrations", "info", "--json"],
    temp.path(),
  )
  .success()
  .stdout(predicate::str::contains(r#""applied":[1]"#));
}

#[test]
fn sqlx_toml_custom_table_is_detected() {
  let temp = assert_fs::TempDir::new().expect("temp dir");
  let db = database(&temp);
  let migrations = temp.path().join("migrations");
  write_migration(
    &migrations,
    "1_init.sql",
    "create table widget (id integer);",
    "drop table widget;",
  );
  fs::write(
    temp.path().join("sqlx.toml"),
    "[migrate]\ntable-name = \"custom_migrations\"\n",
  )
  .expect("write sqlx.toml");

  let mut backend = Sqlite::new(&db).expect("open sqlite");
  backend
    .execute_batch(
      "create table custom_migrations (
         version integer not null primary key,
         description text not null,
         installed_on text not null default (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
         success boolean not null,
         checksum blob not null,
         execution_time integer not null
       );",
    )
    .expect("create custom table");

  common::run_midas(&["--database", &db, "doctor", "--sqlx"], temp.path())
    .success()
    .stdout(predicate::str::contains("custom_migrations"));
}
