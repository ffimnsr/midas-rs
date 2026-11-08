mod common;

use std::fs;

use assert_fs::prelude::PathChild as _;
use assert_fs::TempDir;
use predicates::prelude::*;

fn write_migrations(temp: &TempDir, migrations: &[(&str, &str, &str)]) {
  let dir = temp.child("migrations");
  fs::create_dir_all(dir.path()).expect("create migrations");
  for (name, up, down) in migrations {
    let contents = format!("-- !UP\n{up}\n-- !DOWN\n{down}\n");
    fs::write(dir.path().join(name), contents).expect("write migration");
  }
}

fn database(temp: &TempDir) -> String {
  temp.child("app.db3").path().to_string_lossy().into_owned()
}

#[test]
fn run_applies_migrations_and_info_reports_json() {
  let temp = TempDir::new().expect("temp dir");
  write_migrations(
    &temp,
    &[
      (
        "1_init.sql",
        "create table users (id integer primary key);",
        "drop table users;",
      ),
      (
        "2_posts.sql",
        "create table posts (id integer primary key);",
        "drop table posts;",
      ),
    ],
  );

  common::run_midas(
    &["--database", &database(&temp), "--source", "migrations", "run"],
    temp.path(),
  )
  .success();

  common::run_midas(
    &[
      "--database",
      &database(&temp),
      "--source",
      "migrations",
      "info",
      "--json",
    ],
    temp.path(),
  )
  .success()
  .stdout(predicate::str::contains(
    r#"{"version":1,"applied":[1,2],"pending":[],"dirty":null}"#,
  ));
}

#[test]
fn run_target_applies_only_the_requested_range() {
  let temp = TempDir::new().expect("temp dir");
  write_migrations(
    &temp,
    &[
      ("1_init.sql", "create table one (id integer);", "drop table one;"),
      ("2_two.sql", "create table two (id integer);", "drop table two;"),
      (
        "3_three.sql",
        "create table three (id integer);",
        "drop table three;",
      ),
    ],
  );

  common::run_midas(
    &[
      "--database",
      &database(&temp),
      "--source",
      "migrations",
      "run",
      "--target",
      "2",
      "--json",
    ],
    temp.path(),
  )
  .success()
  .stdout(predicate::eq(
    "{\"version\":1,\"applied\":[1,2],\"pending\":[3],\"dirty\":null}\n",
  ));
}

#[test]
fn dry_run_does_not_apply_anything() {
  let temp = TempDir::new().expect("temp dir");
  write_migrations(
    &temp,
    &[("1_init.sql", "create table dry (id integer);", "drop table dry;")],
  );

  common::run_midas(
    &[
      "--database",
      &database(&temp),
      "--source",
      "migrations",
      "--dry-run",
      "run",
    ],
    temp.path(),
  )
  .success()
  .stdout(predicate::str::contains("-- apply 0000000000001"));

  common::run_midas(
    &[
      "--database",
      &database(&temp),
      "--source",
      "migrations",
      "info",
      "--json",
    ],
    temp.path(),
  )
  .success()
  .stdout(predicate::str::contains(
    r#"{"version":1,"applied":[],"pending":[1],"dirty":null}"#,
  ));
}

#[test]
fn revert_to_and_baseline_update_metadata() {
  let temp = TempDir::new().expect("temp dir");
  write_migrations(
    &temp,
    &[
      ("1_init.sql", "create table one (id integer);", "drop table one;"),
      ("2_two.sql", "create table two (id integer);", "drop table two;"),
    ],
  );

  common::run_midas(
    &["--database", &database(&temp), "--source", "migrations", "run"],
    temp.path(),
  )
  .success();

  common::run_midas(
    &[
      "--database",
      &database(&temp),
      "--source",
      "migrations",
      "revert",
      "--to",
      "1",
    ],
    temp.path(),
  )
  .success();

  common::run_midas(
    &[
      "--database",
      &database(&temp),
      "--source",
      "migrations",
      "info",
      "--json",
    ],
    temp.path(),
  )
  .success()
  .stdout(predicate::str::contains(
    r#"{"version":1,"applied":[1],"pending":[2],"dirty":null}"#,
  ));

  common::run_midas(
    &[
      "--database",
      &database(&temp),
      "--source",
      "migrations",
      "baseline",
      "--to",
      "2",
    ],
    temp.path(),
  )
  .success();

  common::run_midas(
    &[
      "--database",
      &database(&temp),
      "--source",
      "migrations",
      "info",
      "--json",
    ],
    temp.path(),
  )
  .success()
  .stdout(predicate::str::contains(
    r#"{"version":1,"applied":[1,2],"pending":[],"dirty":null}"#,
  ));
}

#[test]
fn dirty_migration_exits_with_code_three() {
  let temp = TempDir::new().expect("temp dir");
  write_migrations(
    &temp,
    &[(
      "1_bad.sql",
      "create table broken (id integer);\ncreate table broken (id integer);",
      "drop table broken;",
    )],
  );

  common::run_midas(
    &["--database", &database(&temp), "--source", "migrations", "run"],
    temp.path(),
  )
  .code(2);

  common::run_midas(
    &["--database", &database(&temp), "--source", "migrations", "run"],
    temp.path(),
  )
  .code(3)
  .stderr(predicate::str::contains("marked dirty"));
}

#[test]
fn db_create_from_migrations_applies_the_schema() {
  let temp = TempDir::new().expect("temp dir");
  write_migrations(
    &temp,
    &[(
      "1_init.sql",
      "create table created (id integer);",
      "drop table created;",
    )],
  );

  common::run_midas(
    &[
      "--database",
      &database(&temp),
      "db",
      "create",
      "--from-migrations",
      "migrations",
    ],
    temp.path(),
  )
  .success();

  common::run_midas(
    &[
      "--database",
      &database(&temp),
      "--source",
      "migrations",
      "info",
      "--json",
    ],
    temp.path(),
  )
  .success()
  .stdout(predicate::str::contains(r#""applied":[1]"#));
}

#[test]
fn no_lock_still_applies_migrations() {
  let temp = TempDir::new().expect("temp dir");
  write_migrations(
    &temp,
    &[(
      "1_init.sql",
      "create table unlocked (id integer);",
      "drop table unlocked;",
    )],
  );

  common::run_midas(
    &[
      "--database",
      &database(&temp),
      "--source",
      "migrations",
      "--no-lock",
      "run",
    ],
    temp.path(),
  )
  .success();
}

#[test]
fn add_rejects_path_traversal() {
  let temp = TempDir::new().expect("temp dir");
  common::run_midas(&["add", "../../etc/passwd"], temp.path()).failure();
  assert!(!temp.child("migrations").exists());
}

#[test]
fn add_creates_a_migration_file() {
  let temp = TempDir::new().expect("temp dir");
  common::run_midas(&["add", "create users table"], temp.path()).success();
  assert!(temp.child("migrations").is_dir());
  let entry = fs::read_dir(temp.child("migrations").path())
    .expect("read")
    .filter_map(Result::ok)
    .find(|entry| {
      entry
        .file_name()
        .to_string_lossy()
        .ends_with("_create_users_table.sql")
    });
  assert!(entry.is_some());
}
