//! Phase 1 migration engine tests backed by a real SQLite database.

mod common;

#[cfg(feature = "sqlite")]
mod sqlite {
  use std::sync::{
    Arc,
    Barrier,
  };
  use std::time::Duration;

  use midas::commander::{
    Migrator,
    MigratorOptions,
  };
  use midas::config::Config;
  use midas::migration::{
    Migration,
    MigrationFormat,
    MigrationList,
    MigrationRecord,
  };
  use midas::sequel::sqlite::{
    Sqlite,
    SqliteAdmin,
  };
  use midas::sequel::{
    self,
    DatabaseAdmin,
    MigrationStore,
  };
  use midas::Error;

  use super::common;

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

  fn config(database_url: &str) -> Config {
    Config {
      database_url: Some(database_url.to_string()),
      lock_timeout: Duration::from_secs(5),
      ..Config::default()
    }
  }

  fn migrator(database_url: &str, migrations: &[Migration]) -> Migrator {
    let backend = sequel::connect(&config(database_url)).expect("connect");
    Migrator::new(backend, list(migrations))
  }

  fn table_exists(database_url: &str, name: &str) -> bool {
    let connection = rusqlite::Connection::open(database_url).expect("open probe");
    connection
      .query_row(
        "select exists(select 1 from sqlite_master where type = 'table' and name = ?1)",
        [name],
        |row| row.get(0),
      )
      .expect("query")
  }

  #[test]
  fn fresh_metadata_table_has_v2_columns() {
    let database = common::run_sqlite();
    let _backend = Sqlite::new(database.url()).expect("open");

    let connection = rusqlite::Connection::open(database.url()).expect("open probe");
    let mut statement = connection
      .prepare("select name, type from pragma_table_info('__schema_migrations')")
      .expect("prepare");
    let columns = statement
      .query_map((), |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
      .expect("query")
      .collect::<rusqlite::Result<Vec<_>>>()
      .expect("collect");

    for expected in ["installed_on", "down_checksum", "checksum"] {
      assert!(
        columns.iter().any(|(name, _)| name == expected),
        "missing column `{expected}`"
      );
    }
    assert!(columns
      .iter()
      .any(|(name, kind)| name == "checksum" && kind == "BLOB"));
  }

  #[test]
  fn legacy_table_is_upgraded_and_rows_preserved() {
    let database = common::run_sqlite();
    {
      let connection = rusqlite::Connection::open(database.url()).expect("open");
      connection
        .execute_batch(
          "CREATE TABLE __schema_migrations (
             version INTEGER NOT NULL PRIMARY KEY,
             description TEXT NOT NULL,
             checksum TEXT NOT NULL,
             execution_time_ms INTEGER NOT NULL,
             success BOOLEAN NOT NULL
           );
           INSERT INTO __schema_migrations VALUES (1, '', '', 5, 1);
           INSERT INTO __schema_migrations VALUES (2, '', '', 7, 1);",
        )
        .expect("legacy schema");
    }

    let mut backend = Sqlite::new(database.url()).expect("upgrade");
    let records = backend.list_migrations().expect("list");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].version, 1);
    assert_eq!(records[1].version, 2);
    assert_eq!(records[0].execution_time, 5_000_000);
    assert_eq!(records[1].execution_time, 7_000_000);
    assert!(records.iter().all(|record| record.success));
    assert!(records.iter().all(|record| record.checksum == vec![0; 48]));
  }

  #[test]
  fn transactional_failure_rolls_back_and_marks_dirty() {
    let database = common::run_sqlite();
    let mut migrator = migrator(
      database.url(),
      &[migration(
        1,
        "create table widgets (id integer);\ncreate table widgets (id integer);\n",
        Some("drop table widgets;\n"),
        false,
      )],
    );

    let error = migrator.up(None).expect_err("second create must fail");
    assert!(matches!(error, Error::Database(_)));
    assert!(!table_exists(database.url(), "widgets"));
    assert!(matches!(migrator.info().expect("info").dirty, Some(1)));
    assert!(matches!(
      migrator.up(None).expect_err("dirty blocks"),
      Error::DirtyMigration { version: 1 }
    ));

    migrator.resolve_applied(None).expect("resolve applied");
    assert!(migrator.info().expect("info").dirty.is_none());
  }

  #[test]
  fn resolve_rolled_back_removes_the_dirty_row() {
    let database = common::run_sqlite();
    let mut migrator = migrator(database.url(), &[migration(1, "this is not sql;\n", None, true)]);

    migrator.up(None).expect_err("invalid sql");
    assert_eq!(migrator.info().expect("info").dirty, Some(1));

    migrator.resolve_rolled_back(None).expect("resolve rolled back");
    let info = migrator.info().expect("info");
    assert!(info.dirty.is_none());
    assert!(info.applied.is_empty());
  }

  #[test]
  fn no_transaction_failure_leaves_partial_sql_and_dirty_row() {
    let database = common::run_sqlite();
    let mut migrator = migrator(
      database.url(),
      &[migration(
        1,
        "create table partial (id integer);\nthis is not sql;\n",
        None,
        true,
      )],
    );

    migrator.up(None).expect_err("invalid sql");
    assert!(table_exists(database.url(), "partial"));
    assert_eq!(migrator.info().expect("info").dirty, Some(1));
  }

  #[test]
  fn multi_statement_migrations_apply() {
    let database = common::run_sqlite();
    let mut migrator = migrator(
      database.url(),
      &[migration(
        1,
        "create table a (id integer);\ncreate table b (id integer);\ncreate table c (id integer);\n",
        Some("drop table c;\ndrop table b;\ndrop table a;\n"),
        false,
      )],
    );

    migrator.up(None).expect("apply");
    assert!(table_exists(database.url(), "a"));
    assert!(table_exists(database.url(), "b"));
    assert!(table_exists(database.url(), "c"));

    migrator.revert_to(Some(0)).expect("revert");
    assert!(!table_exists(database.url(), "a"));
  }

  #[test]
  fn checksum_mismatch_blocks_and_repair_fixes_it() {
    let database = common::run_sqlite();
    let migration = migration(1, "create table checksummed (id integer);\n", None, false);
    let mut migrator = migrator(database.url(), std::slice::from_ref(&migration));
    migrator.up(None).expect("apply");

    {
      let mut backend = sequel::connect(&config(database.url())).expect("connect");
      let bogus = MigrationRecord::applied(1, "migration_1", vec![9; 48], None, 0);
      backend.upsert(&bogus).expect("corrupt checksum");
    }

    let error = migrator.up(None).expect_err("checksum mismatch");
    assert!(matches!(error, Error::ChecksumMismatch { version: 1, .. }));

    migrator.repair().expect("repair");
    migrator.up(None).expect("up after repair");
  }

  #[test]
  fn legacy_zero_checksums_are_skipped() {
    let database = common::run_sqlite();
    let first = migration(1, "create table legacy (id integer);\n", None, false);
    let mut runner = migrator(database.url(), std::slice::from_ref(&first));
    runner.up(None).expect("apply");

    {
      let mut backend = sequel::connect(&config(database.url())).expect("connect");
      let legacy = MigrationRecord::applied(1, "migration_1", vec![0; 48], None, 0);
      backend.upsert(&legacy).expect("legacy row");
    }

    // The UP text differs, but an all-zero checksum means "legacy/unverified".
    let mut changed = first.clone();
    changed.up = "create table legacy_changed (id integer);\n".to_string();
    changed.checksum_up = midas::migration::checksum(&changed.up);
    let mut migrator = migrator(database.url(), &[changed]);
    migrator.up(None).expect("legacy checksums are skipped");
    migrator.repair().expect("repair legacy row");

    let mut backend = sequel::connect(&config(database.url())).expect("connect");
    let record = backend.list_migrations().expect("list").remove(0);
    assert!(!record.is_unverified(), "repair must populate the checksum");
  }

  #[test]
  fn baseline_marks_versions_without_running_sql() {
    let database = common::run_sqlite();
    let mut migrator = migrator(
      database.url(),
      &[
        migration(1, "create table one (id integer);\n", None, false),
        migration(2, "create table two (id integer);\n", None, false),
      ],
    );

    migrator.baseline(2).expect("baseline");
    assert_eq!(migrator.info().expect("info").applied, vec![1, 2]);
    assert!(!table_exists(database.url(), "one"));
    assert!(!table_exists(database.url(), "two"));
  }

  #[test]
  fn reset_drops_metadata_and_replays() {
    let database = common::run_sqlite();
    let mut migrator = migrator(
      database.url(),
      &[migration(
        1,
        "create table if not exists replay (id integer);\n",
        Some("drop table if exists replay;\n"),
        false,
      )],
    );

    migrator.up(None).expect("apply");
    migrator.reset().expect("reset");
    assert_eq!(migrator.info().expect("info").applied, vec![1]);
    assert!(table_exists(database.url(), "replay"));
  }

  #[test]
  fn dry_run_executes_no_sql() {
    let database = common::run_sqlite();
    let mut migrator = migrator(
      database.url(),
      &[migration(1, "create table dry (id integer);\n", None, false)],
    )
    .with_options(MigratorOptions {
      dry_run: true,
      ..MigratorOptions::default()
    });

    migrator.up(None).expect("dry run");
    assert!(!table_exists(database.url(), "dry"));
    assert!(migrator.info().expect("info").applied.is_empty());
  }

  #[test]
  fn concurrent_runners_apply_exactly_once() {
    let database = common::run_sqlite();
    let config = config(database.url());
    let migrations = list(&[migration(
      1,
      "create table concurrent (id integer);\n",
      Some("drop table concurrent;\n"),
      false,
    )]);

    // Create the metadata table before the race starts.
    drop(sequel::connect(&config).expect("bootstrap"));

    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();
    for _ in 0..2 {
      let config = config.clone();
      let migrations = migrations.clone();
      let barrier = Arc::clone(&barrier);
      handles.push(std::thread::spawn(move || {
        let backend = sequel::connect(&config).expect("connect");
        let mut migrator = Migrator::new(backend, migrations);
        barrier.wait();
        migrator.up(None)
      }));
    }

    for handle in handles {
      handle.join().expect("thread").expect("up");
    }

    let mut backend = sequel::connect(&config).expect("connect");
    let records = backend.list_migrations().expect("list");
    assert_eq!(records.len(), 1);
    assert!(records[0].success);
  }

  #[test]
  fn custom_metadata_table_is_used() {
    let database = common::run_sqlite();
    let config = Config {
      database_url: Some(database.url().to_string()),
      metadata_table: "_sqlx_migrations".to_string(),
      ..Config::default()
    };
    let mut backend = Sqlite::with_config(&config, database.url()).expect("connect");
    assert!(backend.list_migrations().expect("list").is_empty());
    drop(backend);

    let connection = rusqlite::Connection::open(database.url()).expect("open probe");
    let exists: bool = connection
      .query_row(
        "select exists(select 1 from sqlite_master where type = 'table' and name = ?1)",
        ["_sqlx_migrations"],
        |row| row.get(0),
      )
      .expect("query");
    assert!(exists);
  }

  #[test]
  fn sqlite_admin_manages_database_files() {
    let database = common::run_sqlite();
    let path = database.url().to_string();

    let mut admin = SqliteAdmin::new(&path);
    assert!(!admin.database_exists("app").expect("exists"));
    admin.create_database("app").expect("create");
    assert!(admin.database_exists("app").expect("exists"));
    admin.drop_database("app").expect("drop");
    assert!(!admin.database_exists("app").expect("exists"));
  }

  #[test]
  fn sqlite_admin_is_reachable_from_config() {
    let database = common::run_sqlite();
    let mut admin = sequel::connect_admin(&config(database.url())).expect("admin");
    assert!(!admin.database_exists("app").expect("exists"));
    admin.create_database("app").expect("create");
    assert!(admin.database_exists("app").expect("exists"));
    admin.force_drop_database("app").expect("drop");
    assert!(!admin.database_exists("app").expect("exists"));
  }
}
