//! Phase 1 migration engine tests against a real PostgreSQL container.
//!
//! These are ignored by default and run in CI with Docker available.

mod common;

#[cfg(feature = "postgres")]
mod postgres {
  use std::time::Duration;

  use midas::commander::Migrator;
  use midas::config::Config;
  use midas::migration::{
    Migration,
    MigrationFormat,
    MigrationList,
  };
  use midas::sequel::postgres::Postgres;
  use midas::sequel::{
    self,
    MigrationStore,
  };
  use midas::Error;
  use postgres::{
    Client,
    NoTls,
  };

  use super::common;

  fn config(database_url: &str) -> Config {
    Config {
      database_url: Some(database_url.to_string()),
      lock_timeout: Duration::from_secs(5),
      ..Config::default()
    }
  }

  fn migration(version: i64, up: &str, down: Option<&str>, no_tx: bool) -> Migration {
    Migration::from_parts(
      version,
      format!("migration_{version}"),
      up,
      down.map(ToString::to_string),
      no_tx,
      format!("{version:013}_migration_{version}.sql"),
      MigrationFormat::Midas,
    )
  }

  fn list(migrations: &[Migration]) -> MigrationList {
    migrations
      .iter()
      .cloned()
      .map(|migration| (migration.version, migration))
      .collect()
  }

  fn client(database_url: &str) -> Client {
    Client::connect(database_url, NoTls).expect("connect admin client")
  }

  #[test]
  #[ignore = "requires Docker"]
  fn fresh_metadata_table_matches_sqlx_columns() {
    let database = common::run_postgres();
    let _backend = Postgres::new(database.url()).expect("connect");

    let mut client = client(database.url());
    let rows = client
      .query(
        "select column_name from information_schema.columns \
         where table_schema = 'midas' and table_name = '__schema_migrations'",
        &[],
      )
      .expect("columns");
    let columns = rows.iter().map(|row| row.get::<_, String>(0)).collect::<Vec<_>>();
    for expected in [
      "version",
      "description",
      "installed_on",
      "success",
      "checksum",
      "execution_time",
      "down_checksum",
    ] {
      assert!(
        columns.iter().any(|column| column == expected),
        "missing {expected}"
      );
    }
  }

  #[test]
  #[ignore = "requires Docker"]
  fn one_column_legacy_table_is_upgraded() {
    let database = common::run_postgres();
    {
      let mut client = client(database.url());
      client
        .batch_execute(
          "create schema if not exists midas;
           create table midas.__schema_migrations (migration bigint primary key);
           insert into midas.__schema_migrations (migration) values (1), (2);",
        )
        .expect("legacy table");
    }

    let mut backend = Postgres::new(database.url()).expect("upgrade");
    let records = backend.list_migrations().expect("list");
    assert_eq!(
      records.iter().map(|record| record.version).collect::<Vec<_>>(),
      vec![1, 2]
    );
    assert!(records.iter().all(|record| record.success));
    assert!(records.iter().all(|record| record.checksum == vec![0; 48]));
    assert!(records.iter().all(|record| record.execution_time == -1));
  }

  #[test]
  #[ignore = "requires Docker"]
  fn multi_statement_and_dollar_quoted_migrations_apply() {
    let database = common::run_postgres();
    let mut migrator = Migrator::new(
      sequel::connect(&config(database.url())).expect("connect"),
      list(&[migration(
        1,
        "create table multi_a (id integer);\n\
         create table multi_b (id integer);\n\
         create function fn_multi() returns integer as $$\nbegin\n  return 42;\nend;\n$$ language plpgsql;\n",
        Some("drop function fn_multi();\ndrop table multi_b;\ndrop table multi_a;\n"),
        false,
      )]),
    );

    migrator.up(None).expect("apply");
    let mut query_client = client(database.url());
    let row = query_client
      .query_one("select fn_multi()", &[])
      .expect("call function");
    assert_eq!(row.get::<_, i32>(0), 42);

    migrator.revert_to(Some(0)).expect("revert");
    let dropped: bool = query_client
      .query_one("select to_regclass('public.multi_a') is null", &[])
      .expect("query")
      .get(0);
    assert!(dropped);
  }

  #[test]
  #[ignore = "requires Docker"]
  fn failed_transaction_rolls_back_and_marks_dirty() {
    let database = common::run_postgres();
    let mut migrator = Migrator::new(
      sequel::connect(&config(database.url())).expect("connect"),
      list(&[migration(
        1,
        "create table rollback_target (id integer);\ncreate table rollback_target (id integer);\n",
        Some("drop table rollback_target;\n"),
        false,
      )]),
    );

    let error = migrator.up(None).expect_err("duplicate table");
    assert!(matches!(error, Error::Database(_)));
    assert_eq!(migrator.info().expect("info").dirty, Some(1));

    let mut client = client(database.url());
    let exists: Option<String> = client
      .query_one("select to_regclass('public.rollback_target')::text", &[])
      .expect("query")
      .get(0);
    assert!(exists.is_none(), "DDL must roll back");

    migrator.resolve_applied(None).expect("resolve");
    assert!(migrator.info().expect("info").dirty.is_none());
  }

  #[test]
  #[ignore = "requires Docker"]
  fn second_runner_times_out_on_the_advisory_lock() {
    let database = common::run_postgres();
    let config = Config {
      lock_timeout: Duration::from_millis(200),
      ..config(database.url())
    };

    let mut first = sequel::connect(&config).expect("first");
    let mut second = sequel::connect(&config).expect("second");
    first.lock().expect("first lock");
    let error = second.lock().expect_err("second must time out");
    assert!(matches!(error, Error::LockTimeout { backend: "postgres" }));
    first.unlock().expect("unlock");
  }

  #[test]
  #[ignore = "requires Docker"]
  fn phase0_metadata_table_is_upgraded() {
    let database = common::run_postgres();
    {
      let mut client = client(database.url());
      client
        .batch_execute(
          "create schema if not exists midas;
           create table midas.__schema_migrations (
             version bigint primary key,
             description text not null,
             checksum text not null,
             execution_time_ms bigint not null,
             success boolean not null
           );
           insert into midas.__schema_migrations values (1, '', '', 10, true);",
        )
        .expect("phase 0 table");
    }

    let mut backend = Postgres::new(database.url()).expect("upgrade");
    let records = backend.list_migrations().expect("list");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].execution_time, 10_000_000);
    assert_eq!(records[0].checksum, vec![0; 48]);

    let mut client = client(database.url());
    let legacy_column: bool = client
      .query_one(
        "select exists(select 1 from information_schema.columns \
         where table_schema = 'midas' and table_name = '__schema_migrations' \
         and column_name = 'execution_time_ms')",
        &[],
      )
      .expect("query")
      .get(0);
    assert!(!legacy_column, "legacy column must be dropped");
  }

  #[test]
  #[ignore = "requires Docker"]
  fn created_database_can_run_migrations() {
    let database = common::run_postgres();
    let mut admin = sequel::connect_admin(&config(database.url())).expect("admin");
    let name = common::unique_name("midas_app");
    admin.create_database(&name).expect("create");

    let target = format!("{}/{}", database.url().trim_end_matches("/postgres"), name);
    let mut migrator = Migrator::new(
      sequel::connect(&config(&target)).expect("connect target"),
      list(&[migration(
        1,
        "create table created_app (id integer);\n",
        Some("drop table created_app;\n"),
        false,
      )]),
    );
    migrator.up(None).expect("apply");
    assert_eq!(migrator.info().expect("info").applied, vec![1]);

    admin.force_drop_database(&name).expect("drop");
  }

  #[test]
  #[ignore = "requires Docker"]
  fn admin_can_create_and_drop_databases() {
    let database = common::run_postgres();
    let mut admin = sequel::connect_admin(&config(database.url())).expect("admin");
    let name = common::unique_name("midas_created");

    assert!(!admin.database_exists(&name).expect("exists"));
    admin.create_database(&name).expect("create");
    assert!(admin.database_exists(&name).expect("exists"));
    admin.force_drop_database(&name).expect("drop");
    assert!(!admin.database_exists(&name).expect("exists"));
  }
}
