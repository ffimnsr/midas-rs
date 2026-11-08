//! SQLite harness smoke test.

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
