//! Shared test harness for database-backed tests.

#![allow(dead_code)]

use std::path::{
  Path,
  PathBuf,
};
use std::sync::atomic::{
  AtomicU64,
  Ordering,
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Build a process-unique name without relying on the wall clock.
pub fn unique_name(prefix: &str) -> String {
  let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
  format!("{prefix}_{}_{}", std::process::id(), counter)
}

/// Temporary directory that removes itself on drop.
#[derive(Debug)]
pub struct TempDir {
  path: PathBuf,
}

impl TempDir {
  /// Create a temporary directory below the system temp directory.
  pub fn new(prefix: &str) -> std::io::Result<Self> {
    let path = std::env::temp_dir().join(unique_name(prefix));
    std::fs::create_dir_all(&path)?;
    Ok(Self { path })
  }

  /// Directory path.
  pub fn path(&self) -> &Path {
    &self.path
  }
}

impl Drop for TempDir {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.path);
  }
}

/// Database handle that cleans up its resources on drop.
pub struct TempDb {
  url: String,
  path: Option<PathBuf>,
  _directory: Option<TempDir>,
  _container: Option<Box<dyn std::any::Any>>,
}

impl TempDb {
  /// Connection URL for the temporary database.
  pub fn url(&self) -> &str {
    &self.url
  }

  fn sqlite() -> std::io::Result<Self> {
    let directory = TempDir::new("midas-sqlite")?;
    let path = directory.path().join("test.db3");
    Ok(Self {
      url: path.to_string_lossy().into_owned(),
      path: Some(path),
      _directory: Some(directory),
      _container: None,
    })
  }
}

impl Drop for TempDb {
  fn drop(&mut self) {
    if let Some(path) = &self.path {
      let _ = std::fs::remove_file(path);
      let _ = std::fs::remove_file(format!("{}-wal", path.to_string_lossy()));
      let _ = std::fs::remove_file(format!("{}-shm", path.to_string_lossy()));
    }
  }
}

/// Start a temporary SQLite database.
#[cfg(feature = "sqlite")]
pub fn run_sqlite() -> TempDb {
  TempDb::sqlite().expect("create sqlite temp database")
}

/// Start a temporary PostgreSQL container.
#[cfg(feature = "postgres")]
pub fn run_postgres() -> TempDb {
  use testcontainers::runners::SyncRunner as _;
  use testcontainers_modules::postgres::Postgres;

  let container = Postgres::default()
    .start()
    .expect("start postgres test container");
  let host = container.get_host().expect("postgres container host");
  let port = container
    .get_host_port_ipv4(5432)
    .expect("postgres container port");
  TempDb {
    url: format!("postgres://postgres:postgres@{host}:{port}/postgres"),
    path: None,
    _directory: None,
    _container: Some(Box::new(container)),
  }
}

/// Start a temporary PostgreSQL container.
#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn unique_names_do_not_collide() {
    let first = unique_name("midas");
    let second = unique_name("midas");
    assert_ne!(first, second);
    assert!(first.starts_with("midas_"));
    assert!(first.contains(&std::process::id().to_string()));
  }

  #[test]
  fn temp_dir_removes_itself() {
    let path = {
      let directory = TempDir::new("midas-temp").expect("temp dir");
      let path = directory.path().to_path_buf();
      assert!(path.exists());
      path
    };
    assert!(!path.exists());
  }
}
