mod common;

use std::fs;

use assert_fs::prelude::PathChild as _;
use assert_fs::TempDir;
use predicates::prelude::*;

#[test]
fn it_should_show_help() {
  let temp = TempDir::new().expect("temp dir");
  common::run_midas(&["--help"], temp.path())
    .success()
    .stdout(predicate::str::contains("Do painless migration 🦀"))
    .stdout(predicate::str::contains("init"))
    .stdout(predicate::str::contains("create"))
    .stdout(predicate::str::contains("status"))
    .stdout(predicate::str::contains("list"))
    .stdout(predicate::str::contains("up"))
    .stdout(predicate::str::contains("upto"))
    .stdout(predicate::str::contains("down"))
    .stdout(predicate::str::contains("revert"))
    .stdout(predicate::str::contains("redo"))
    .stdout(predicate::str::contains("drop"))
    .stdout(predicate::str::contains("completion"))
    .stdout(predicate::str::contains("--no-color"))
    .stdout(predicate::str::contains("--database"))
    .stdout(predicate::str::contains("--source"))
    .stdout(predicate::str::contains("--help"))
    .stdout(predicate::str::contains("--version"))
    .stdout(predicate::str::contains("faker").not())
    .stdout(predicate::str::contains("seed").not())
    .stdout(predicate::str::contains("update").not());
}

#[test]
fn it_should_show_version() {
  let temp = TempDir::new().expect("temp dir");
  common::run_midas(&["--version"], temp.path())
    .success()
    .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn it_should_show_init_help() {
  let temp = TempDir::new().expect("temp dir");
  common::run_midas(&["init", "--help"], temp.path())
    .success()
    .stdout(predicate::str::contains("Set up a midas project"))
    .stdout(predicate::str::contains("--force"));
}

#[test]
fn init_creates_only_config_and_migrations() {
  let temp = TempDir::new().expect("temp dir");
  common::run_midas(&["init"], temp.path()).success();

  let project = temp.child("midas.toml");
  let schema = temp.child("schema.toml");
  let migrations = temp.child("migrations");

  assert!(project.is_file());
  assert!(schema.is_file());
  assert!(migrations.is_dir());

  let entries = fs::read_dir(temp.path())
    .expect("read temp dir")
    .filter_map(Result::ok)
    .map(|entry| entry.file_name().to_string_lossy().into_owned())
    .collect::<Vec<_>>();
  let mut entries = entries;
  entries.sort();

  assert_eq!(
    entries,
    vec![
      "midas.toml".to_string(),
      "migrations".to_string(),
      "schema.toml".to_string()
    ]
  );
  assert!(!temp.child(".env.midas").exists());
  assert!(!temp.child(".migrations-state").exists());
  assert!(!temp.child("data.db3").exists());
}

#[test]
fn second_init_fails_without_force_and_keeps_files() {
  let temp = TempDir::new().expect("temp dir");
  common::run_midas(&["init"], temp.path()).success();

  let project = temp.child("midas.toml");
  let schema = temp.child("schema.toml");
  let before_project = fs::read_to_string(project.path()).expect("read project");
  let before_schema = fs::read_to_string(schema.path()).expect("read schema");

  common::run_midas(&["init"], temp.path())
    .failure()
    .stderr(predicate::str::contains("already exists"));

  assert_eq!(
    fs::read_to_string(project.path()).expect("read project"),
    before_project
  );
  assert_eq!(
    fs::read_to_string(schema.path()).expect("read schema"),
    before_schema
  );
}

#[test]
fn init_force_overwrites_files() {
  let temp = TempDir::new().expect("temp dir");
  common::run_midas(&["init"], temp.path()).success();

  let project = temp.child("midas.toml");
  fs::write(project.path(), "broken = true\n").expect("write project");

  common::run_midas(&["init", "--force"], temp.path()).success();
  let contents = fs::read_to_string(project.path()).expect("read project");
  assert!(contents.contains("[project]"));
}

#[test]
fn status_without_database_url_fails_with_config_error() {
  let temp = TempDir::new().expect("temp dir");
  common::run_midas(&["status"], temp.path())
    .failure()
    .stderr(predicate::str::contains("configuration error"))
    .stderr(predicate::str::contains("no database URL"));
}

#[test]
fn create_writes_a_timestamped_migration() {
  let temp = TempDir::new().expect("temp dir");
  common::run_midas(&["create", "create_users_table"], temp.path()).success();

  let migrations = temp.child("migrations");
  assert!(migrations.is_dir());

  let entry = fs::read_dir(migrations.path())
    .expect("read migrations")
    .filter_map(Result::ok)
    .find(|entry| {
      entry
        .file_name()
        .to_string_lossy()
        .ends_with("_create_users_table.sql")
    });
  assert!(entry.is_some());
}

#[test]
fn create_slugifies_spaces() {
  let temp = TempDir::new().expect("temp dir");
  common::run_midas(&["create", "Create users table with space"], temp.path()).success();

  let migrations = temp.child("migrations");
  let entry = fs::read_dir(migrations.path())
    .expect("read migrations")
    .filter_map(Result::ok)
    .find(|entry| {
      entry
        .file_name()
        .to_string_lossy()
        .ends_with("_create_users_table_with_space.sql")
    });
  assert!(entry.is_some());
}

#[test]
fn source_flag_controls_migrations_directory() {
  let temp = TempDir::new().expect("temp dir");
  common::run_midas(&["--source", "db/migrations", "create", "initial"], temp.path()).success();
  assert!(temp.child("db/migrations").is_dir());
}

#[test]
fn list_reads_migrations_without_a_database() {
  let temp = TempDir::new().expect("temp dir");
  let migrations = temp.path().join("migrations");
  common::write_migration(
    &migrations,
    "initial",
    "create table t (id int);",
    "drop table t;",
  );

  common::run_midas(&["list"], temp.path())
    .success()
    .stdout(predicate::str::contains("initial"));
}

#[test]
fn write_migration_helper_is_usable() {
  let temp = TempDir::new().expect("temp dir");
  let migrations = temp.path().join("migrations");
  common::write_migration(
    &migrations,
    "initial",
    "create table t (id int);",
    "drop table t;",
  );

  let entry = fs::read_dir(&migrations)
    .expect("read migrations")
    .filter_map(Result::ok)
    .next()
    .expect("one migration");
  let contents = fs::read_to_string(entry.path()).expect("read migration");
  assert!(contents.starts_with("-- !UP\n"));
  assert!(contents.contains("-- !DOWN"));
}
