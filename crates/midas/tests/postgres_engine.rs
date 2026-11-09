//! Phase 1 migration engine tests against a real PostgreSQL container.
//!
//! These are ignored by default and run in CI with Docker available.

mod common;

#[cfg(feature = "postgres")]
mod postgres {
  use std::collections::BTreeSet;
  use std::fs;
  use std::time::Duration;

  use midas::commander::{
    Migrator,
    MigratorOptions,
  };
  use midas::config::Config;
  use midas::migration::{
    checksum_hex,
    ChecksumMode,
    Migration,
    MigrationFormat,
    MigrationList,
  };
  use midas::sequel::postgres::Postgres;
  use midas::sequel::{
    self,
    MigrationStore,
  };
  use midas::sqlx::baseline_from_sqlx;
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

  fn sqlx_checksum(sql: &str) -> Vec<u8> {
    ChecksumMode::Sqlx {
      ignored_chars: Default::default(),
    }
    .checksum(sql)
    .to_vec()
  }

  fn create_sqlx_table(database_url: &str, up: &str, success: bool, execution_time: i64) -> Vec<u8> {
    let checksum = sqlx_checksum(up);
    let mut client = client(database_url);
    client
      .batch_execute(&format!(
        "drop table if exists public._sqlx_migrations;
         create table public._sqlx_migrations (
           version bigint primary key,
           description text not null,
           installed_on timestamptz not null default now(),
           success boolean not null,
           checksum bytea not null,
           execution_time bigint not null
         );
         insert into public._sqlx_migrations
           (version, description, success, checksum, execution_time)
         values (1, 'init', {success}, decode('{}', 'hex'), {execution_time});",
        checksum_hex(&checksum)
      ))
      .expect("create sqlx metadata table");
    checksum
  }

  fn adoption_config(database_url: &str) -> Config {
    Config {
      database_url: Some(database_url.to_string()),
      metadata_schema: None,
      metadata_table: "_sqlx_migrations".to_string(),
      lock_timeout: Duration::from_secs(5),
      ..Config::default()
    }
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
  fn sqlx_table_is_adopted_without_alteration() {
    let database = common::run_postgres();
    let up = "create table adopted_widget (id integer);\n";
    create_sqlx_table(database.url(), up, true, 7);

    let config = adoption_config(database.url());
    let mut backend = sequel::connect(&config).expect("adopt");
    let info = backend.metadata_info().expect("metadata info");
    assert!(info.sqlx_managed);
    assert!(!info.has_down_checksum());

    let mut migrator = Migrator::new(
      sequel::connect(&config).expect("connect"),
      list(&[migration(1, up, Some("drop table adopted_widget;\n"), false)]),
    );
    let state = migrator.up(None).expect("up is a no-op");
    assert_eq!(state.applied, vec![1]);

    let mut client = client(database.url());
    let down_checksum: bool = client
      .query_one(
        "select exists(select 1 from information_schema.columns \
         where table_schema = 'public' and table_name = '_sqlx_migrations' \
         and column_name = 'down_checksum')",
        &[],
      )
      .expect("query")
      .get(0);
    assert!(!down_checksum, "adoption must not add columns");
  }

  #[test]
  #[ignore = "requires Docker"]
  fn sqlx_dirty_rows_are_rejected() {
    let database = common::run_postgres();
    create_sqlx_table(database.url(), "select 1;\n", false, 1);

    let config = adoption_config(database.url());
    let error = midas::sqlx::verify_adoption(&config, "_sqlx_migrations").expect_err("dirty");
    assert!(matches!(error, Error::DirtyMigration { version: 1 }));
  }

  #[test]
  #[ignore = "requires Docker"]
  fn baseline_from_sqlx_never_runs_sql() {
    let database = common::run_postgres();
    let up = "this is not valid sql;\n";
    create_sqlx_table(database.url(), up, true, 42);

    let migrations_dir = tempfile::tempdir().expect("temp dir");
    fs::write(
      migrations_dir.path().join("1_init.sql"),
      "-- !UP\nthis is not valid sql;\n-- !DOWN\nselect 1;\n",
    )
    .expect("write migration");

    let config = Config {
      database_url: Some(database.url().to_string()),
      migrations_dir: migrations_dir.path().to_path_buf(),
      lock_timeout: Duration::from_secs(5),
      ..Config::default()
    };
    let report = baseline_from_sqlx(&config, &MigratorOptions::default()).expect("baseline");
    assert_eq!(report.inserted, vec![1]);

    let mut backend = sequel::connect_existing(&config).expect("target");
    let records = backend.list_migrations().expect("list");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].execution_time, 42);
    assert_eq!(
      records[0].checksum,
      ChecksumMode::Midas.checksum("this is not valid sql;\n"),
      "target checksums use the Midas contract"
    );
  }

  #[test]
  #[ignore = "requires Docker"]
  fn execution_time_is_recorded_in_nanoseconds() {
    let database = common::run_postgres();
    let mut migrator = Migrator::new(
      sequel::connect(&config(database.url())).expect("connect"),
      list(&[migration(
        1,
        "select pg_sleep(0.01);\n",
        Some("select 1;\n"),
        false,
      )]),
    );
    migrator.up(None).expect("apply");

    let mut backend = sequel::connect_existing(&config(database.url())).expect("connect");
    let record = &backend.list_migrations().expect("list")[0];
    assert!(
      record.execution_time > 1_000_000,
      "execution time must be nanoseconds, got {}",
      record.execution_time
    );
  }

  #[test]
  #[ignore = "requires Docker"]
  fn create_schemas_places_the_metadata_table() {
    let database = common::run_postgres();
    let config = Config {
      database_url: Some(database.url().to_string()),
      metadata_schema: Some("audit_schema".to_string()),
      metadata_table: "migrations".to_string(),
      create_schemas: ["audit_schema".to_string()].into_iter().collect::<BTreeSet<_>>(),
      lock_timeout: Duration::from_secs(5),
      ..Config::default()
    };
    let _backend = sequel::connect(&config).expect("connect");

    let mut client = client(database.url());
    let exists: bool = client
      .query_one(
        "select exists(select 1 from information_schema.tables \
         where table_schema = 'audit_schema' and table_name = 'migrations')",
        &[],
      )
      .expect("query")
      .get(0);
    assert!(exists);
  }

  #[test]
  #[ignore = "requires Docker"]
  fn no_lock_applies_with_the_advisory_lock_held() {
    let database = common::run_postgres();
    let config = config(database.url());
    let mut holder = sequel::connect(&config).expect("holder");
    holder.lock().expect("hold lock");

    let mut migrator = Migrator::new(
      sequel::connect(&config).expect("connect"),
      list(&[migration(
        1,
        "create table unlocked (id integer);\n",
        Some("drop table unlocked;\n"),
        false,
      )]),
    )
    .with_options(MigratorOptions {
      no_lock: true,
      ..MigratorOptions::default()
    });
    migrator.up(None).expect("no-lock apply");

    holder.unlock().expect("unlock");
  }
}
