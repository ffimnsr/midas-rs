//! Container smoke tests. These require Docker and are ignored by default.

mod common;

#[cfg(feature = "postgres")]
#[test]
#[ignore = "requires Docker"]
fn postgres_container_creates_and_drops_database() {
  use midas::sequel::postgres::Postgres;
  use midas::sequel::DatabaseAdmin;

  let database = common::run_postgres();
  let mut backend = Postgres::new(database.url()).expect("connect to postgres");
  let name = common::unique_name("midas_pg");

  backend.create_database(&name).expect("create database");
  assert!(backend.database_exists(&name).expect("database exists"));
  backend.force_drop_database(&name).expect("force drop database");
  assert!(!backend.database_exists(&name).expect("database exists"));
}
