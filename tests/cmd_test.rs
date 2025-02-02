use assert_cmd::Command;
use assert_fs::prelude::PathChild as _;
use assert_fs::TempDir;

#[test]
fn test_sanity() {
  assert_eq!(2 + 2, 4);
}

#[test]
fn it_should_show_help() -> anyhow::Result<()> {
  let mut cmd = Command::cargo_bin("midas")?;
  let assert = cmd.arg("--help").assert();
  assert
    .success()
    .stdout(predicates::str::contains("Do painless migration 🦀"))
    .stdout(predicates::str::contains("midas"))
    .stdout(predicates::str::contains("init"))
    .stdout(predicates::str::contains("create"))
    .stdout(predicates::str::contains("status"))
    .stdout(predicates::str::contains("list"))
    .stdout(predicates::str::contains("up"))
    .stdout(predicates::str::contains("upto"))
    .stdout(predicates::str::contains("down"))
    .stdout(predicates::str::contains("revert"))
    .stdout(predicates::str::contains("redo"))
    .stdout(predicates::str::contains("drop"))
    .stdout(predicates::str::contains("seed"))
    .stdout(predicates::str::contains("faker"))
    .stdout(predicates::str::contains("update"))
    .stdout(predicates::str::contains("completion"))
    .stdout(predicates::str::contains("help"))
    .stdout(predicates::str::contains("--database"))
    .stdout(predicates::str::contains("--source"))
    .stdout(predicates::str::contains("--help"))
    .stdout(predicates::str::contains("--version"));

  Ok(())
}

#[test]
fn it_should_show_version() -> anyhow::Result<()> {
  let mut cmd = Command::cargo_bin("midas")?;
  let assert = cmd.arg("--version").assert();
  assert
    .success()
    .stdout(predicates::str::contains(env!("CARGO_PKG_VERSION")));

  Ok(())
}

#[test]
fn it_should_show_init_help() -> anyhow::Result<()> {
  let mut cmd = Command::cargo_bin("midas")?;
  let assert = cmd.arg("init").arg("--help").assert();
  assert
    .success()
    .stdout(predicates::str::contains(
      "Setup and creates initial migration directory and a dotenv file",
    ))
    .stdout(predicates::str::contains("midas init"))
    .stdout(predicates::str::contains("--help"))
    .stdout(predicates::str::contains("--version"));

  Ok(())
}

#[test]
fn it_should_create_files_on_init() -> anyhow::Result<()> {
  let temp_dir = TempDir::new()?;
  let mut cmd = Command::cargo_bin("midas")?;
  let assert = cmd.arg("init").current_dir(temp_dir.path()).assert();
  assert.success();

  let migrations_states_dir = temp_dir.child(".migrations-state");
  let migrations_dir = temp_dir.child("migrations");
  let dotenv_file = temp_dir.child(".env.midas");

  assert!(migrations_states_dir.is_dir());
  assert!(migrations_dir.is_dir());
  assert!(dotenv_file.is_file());

  Ok(())
}

#[test]
fn it_should_create_files_on_create() -> anyhow::Result<()> {
  let temp_dir = TempDir::new()?;
  let mut cmd = Command::cargo_bin("midas")?;
  let assert = cmd
    .arg("create")
    .arg("create_users_table")
    .current_dir(temp_dir.path())
    .assert();
  assert.success();

  let migrations_dir = temp_dir.child("migrations");

  assert!(migrations_dir.exists());
  assert!(migrations_dir.is_dir());

  let entries = std::fs::read_dir(migrations_dir.path())?;
  let migration_file = entries.filter_map(Result::ok).find(|entry| {
    entry
      .file_name()
      .to_string_lossy()
      .ends_with("_create_users_table.sql")
  });

  assert!(migration_file.is_some());
  assert!(migration_file.unwrap().path().is_file());

  Ok(())
}

#[test]
fn it_should_create_files_on_create_with_arg_with_space() -> anyhow::Result<()> {
  let temp_dir = TempDir::new()?;
  let mut cmd = Command::cargo_bin("midas")?;
  let assert = cmd
    .arg("create")
    .arg("Create users table with space")
    .current_dir(temp_dir.path())
    .assert();
  assert.success();

  let migrations_dir = temp_dir.child("migrations");

  assert!(migrations_dir.exists());
  assert!(migrations_dir.is_dir());

  let entries = std::fs::read_dir(migrations_dir.path())?;
  let migration_file = entries.filter_map(Result::ok).find(|entry| {
    entry
      .file_name()
      .to_string_lossy()
      .ends_with("_create_users_table_with_space.sql")
  });

  assert!(migration_file.is_some());
  assert!(migration_file.unwrap().path().is_file());

  Ok(())
}
