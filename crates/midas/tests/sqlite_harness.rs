//! SQLite harness smoke tests.

mod common;

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_temp_database_lifecycle() {
  use midas::sequel::sqlite::Sqlite;
  use midas::sequel::Executor;

  let database = common::run_sqlite();
  let mut backend = Sqlite::new(database.url()).expect("open sqlite database");
  backend
    .execute_batch("create table users (id integer primary key, name text not null);")
    .expect("create table");
  assert!(backend.file_path().ends_with("test.db3"));
}

#[cfg(feature = "sqlite")]
#[test]
fn migrator_runs_against_sqlite_harness() {
  use midas::commander::Migrator;
  use midas::config::Config;
  use midas::lookup::{
    MigrationFile,
    MigrationFiles,
  };
  use midas::sequel::sqlite::Sqlite;
  use midas::sequel::{
    self,
    Executor,
  };

  let database = common::run_sqlite();
  let config = Config {
    database_url: Some(database.url().to_string()),
    ..Config::default()
  };
  let backend = sequel::connect(&config).expect("connect sqlite backend");

  let mut migrations = MigrationFiles::new();
  migrations.insert(
    1,
    MigrationFile {
      content_up: Some(vec![
        "create table users (id integer primary key, name text not null);".to_string(),
      ]),
      content_down: Some(vec!["drop table users;".to_string()]),
      number: 1,
      filename: "0000000000001_create_users.sql".to_string(),
    },
  );

  let mut migrator = Migrator::new(backend, migrations);
  migrator.up().expect("apply migration");
  assert_eq!(migrator.applied().expect("applied").len(), 1);

  let mut probe = Sqlite::new(database.url()).expect("reopen sqlite database");
  probe
    .execute("insert into users (name) values ('ada');")
    .expect("schema was applied");

  migrator.revert().expect("revert migration");
  assert!(migrator.applied().expect("applied").is_empty());
  assert!(
    probe
      .execute("insert into users (name) values ('grace');")
      .is_err(),
    "schema was reverted"
  );
}
