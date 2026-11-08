//! CLI test helpers.

#![allow(dead_code)]

use std::path::Path;
use std::sync::atomic::{
  AtomicI64,
  Ordering,
};
use std::time::{
  SystemTime,
  UNIX_EPOCH,
};

use assert_cmd::assert::Assert;
use assert_cmd::{
  cargo,
  Command,
};

static COUNTER: AtomicI64 = AtomicI64::new(0);

const ENV_KEYS: &[&str] = &[
  "DATABASE_URL",
  "DATABASE_URI",
  "DB_URL",
  "DSN",
  "MIGRATIONS_DIR",
  "MIGRATIONS_ROOT",
  "MIGRATIONS_PATH",
  "MIDAS_DATABASE_URL",
  "MIDAS_MIGRATIONS_DIR",
  "MIDAS_METADATA_SCHEMA",
  "MIDAS_METADATA_TABLE",
  "MIDAS_LOCK_TIMEOUT_MS",
  "MIDAS_STATEMENT_TIMEOUT_MS",
  "MIDAS_TRANSACTION",
  "MIDAS_TLS",
  "MIDAS_APPLICATION_NAME",
  "MIDAS_SEARCH_PATH",
];

/// Run the `midas` binary with a clean environment.
pub fn run_midas(args: &[&str], cwd: &Path) -> Assert {
  let mut command = Command::new(cargo::cargo_bin!("midas"));
  command.args(args).current_dir(cwd);
  for key in ENV_KEYS {
    command.env_remove(key);
  }
  command.assert()
}

/// Write a migration file with canonical markers.
pub fn write_migration(dir: &Path, name: &str, up: &str, down: &str) {
  std::fs::create_dir_all(dir).expect("create migrations directory");
  let filename = format!("{:013}_{name}.sql", next_version());
  let contents = format!("-- !UP\n{up}\n-- !DOWN\n{down}\n");
  std::fs::write(dir.join(filename), contents).expect("write migration");
}

fn next_version() -> i64 {
  let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
  let millis = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|duration| duration.as_millis() as i64)
    .unwrap_or(0);
  // Keep the value 13 digits and unique per call.
  millis.saturating_add(counter)
}
