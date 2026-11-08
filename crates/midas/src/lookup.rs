//! Migration file discovery and creation.

use std::collections::BTreeMap;
use std::fs::{
  self,
  File,
  OpenOptions,
};
use std::io::prelude::*;
use std::io::BufReader;
use std::path::Path;
use std::time::{
  SystemTime,
  UNIX_EPOCH,
};

use indoc::indoc;
use regex::Regex;

use crate::{
  Error,
  Result,
};

/// Convenience alias for migration file contents.
pub type VecStr = Vec<String>;

/// A parsed migration file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationFile {
  /// Lines between `-- !UP` and `-- !DOWN`.
  pub content_up: Option<VecStr>,
  /// Lines after `-- !DOWN`.
  pub content_down: Option<VecStr>,
  /// Migration version parsed from the filename.
  pub number: i64,
  /// Original filename.
  pub filename: String,
}

impl MigrationFile {
  fn new(filename: &str, number: i64) -> Self {
    Self {
      content_up: None,
      content_down: None,
      filename: filename.to_owned(),
      number,
    }
  }

  /// Migration description derived from the filename.
  pub fn description(&self) -> &str {
    self
      .filename
      .split_once('_')
      .map(|(_, rest)| rest)
      .and_then(|rest| rest.strip_suffix(".sql").or(Some(rest)))
      .unwrap_or(&self.filename)
  }

  /// UP SQL with blank lines removed.
  pub fn up_sql(&self) -> String {
    join_content(self.content_up.as_deref())
  }

  /// DOWN SQL with blank lines removed.
  pub fn down_sql(&self) -> String {
    join_content(self.content_down.as_deref())
  }
}

/// Map of migration version to parsed file.
pub type MigrationFiles = BTreeMap<i64, MigrationFile>;

fn join_content(content: Option<&[String]>) -> String {
  content
    .unwrap_or_default()
    .iter()
    .filter(|line| !line.is_empty())
    .cloned()
    .collect::<VecStr>()
    .join("\n")
}

/// Parse a migration filename into its version and name.
fn parse_file(filename: &str) -> Result<MigrationFile> {
  let re = Regex::new(r"^(?P<number>[0-9]{13})_(?P<name>[_0-9a-zA-Z]*)\.sql$")
    .map_err(|error| Error::migration(format!("invalid migration filename pattern: {error}")))?;

  let result = re
    .captures(filename)
    .ok_or_else(|| Error::migration(format!("invalid migration filename `{filename}`")))?;

  let number = result
    .name("number")
    .ok_or_else(|| Error::migration(format!("migration timestamp missing from `{filename}`")))?
    .as_str()
    .parse::<i64>()
    .map_err(|error| Error::migration(format!("invalid migration timestamp in `{filename}`: {error}")))?;

  Ok(MigrationFile::new(filename, number))
}

/// Build the migration list for a directory. Missing directories are empty.
pub fn build_migration_list(path: &Path) -> Result<MigrationFiles> {
  let mut files = MigrationFiles::new();
  if !path.exists() {
    return Ok(files);
  }

  let entries = fs::read_dir(path)?
    .filter_map(|entry| entry.ok())
    .collect::<Vec<_>>();
  for entry in entries {
    if !entry.path().is_file() {
      continue;
    }

    let filename = entry.file_name();
    let Some(filename) = filename.to_str() else {
      continue;
    };
    let Ok(info) = parse_file(filename) else {
      continue;
    };

    let file = File::open(entry.path())?;
    let mut buf_reader = BufReader::new(file);
    let mut content = String::new();
    buf_reader.read_to_string(&mut content)?;

    let split_vec: Vec<String> = content.split('\n').map(ToString::to_string).collect();

    let pos_up = split_vec
      .iter()
      .position(|line| line == "-- !UP" || line == "-- !UP\r")
      .ok_or_else(|| Error::migration(format!("parser cannot find UP marker in `{filename}`")))?;
    let pos_down = split_vec
      .iter()
      .position(|line| line == "-- !DOWN" || line == "-- !DOWN\r")
      .ok_or_else(|| Error::migration(format!("parser cannot find DOWN marker in `{filename}`")))?;

    let content_up = &split_vec[(pos_up + 1)..pos_down];
    let content_down = &split_vec[(pos_down + 1)..];

    let migration = MigrationFile {
      content_up: Some(content_up.to_vec()),
      content_down: Some(content_down.to_vec()),
      ..info
    };

    tracing::trace!(
      filename = %migration.filename,
      number = migration.number,
      "parsed migration"
    );
    let number = migration.number;
    if files.insert(number, migration).is_some() {
      return Err(Error::migration(format!(
        "duplicate migration version {number:013}"
      )));
    }
  }

  Ok(files)
}

fn timestamp() -> Result<String> {
  let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_err(|_| Error::migration("system clock is before the Unix epoch"))?;
  Ok(now.as_millis().to_string())
}

/// Create a new migration file with the canonical markers.
pub fn create_migration_file(path: &Path, slug: &str) -> Result<()> {
  let filename = format!("{}_{}.sql", timestamp()?, slug);
  let filepath = path.join(filename);

  fs::create_dir_all(path)?;
  tracing::trace!(path = %filepath.to_string_lossy(), "creating migration file");
  // `create_new` prevents two migrations created in the same millisecond from
  // silently truncating each other.
  let mut file = OpenOptions::new()
    .write(true)
    .create_new(true)
    .open(&filepath)
    .map_err(|error| {
      if error.kind() == std::io::ErrorKind::AlreadyExists {
        Error::migration(format!(
          "migration file `{}` already exists",
          filepath.to_string_lossy()
        ))
      } else {
        Error::Io(error)
      }
    })?;
  let contents = indoc! {"\
    -- # Put your SQL below migration seperator.
    -- !UP

    -- !DOWN
  "};

  file.write_all(contents.as_bytes())?;
  file.sync_all()?;

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn it_should_parse_correct_migration_filename() {
    let result = parse_file("0000000000000_initial.sql").expect("valid filename");
    assert_eq!(result.number, 0);
    assert_eq!(result.filename, "0000000000000_initial.sql");
  }

  #[test]
  fn invalid_filenames_are_rejected() {
    assert!(parse_file("initial.sql").is_err());
    assert!(parse_file("123_initial.sql").is_err());
  }

  #[test]
  fn missing_directory_yields_empty_list() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("does-not-exist");
    assert!(build_migration_list(&path).expect("list").is_empty());
  }

  #[test]
  fn migration_files_are_parsed() {
    let dir = tempfile::tempdir().expect("temp dir");
    create_migration_file(dir.path(), "create_users").expect("create");
    let migrations = build_migration_list(dir.path()).expect("list");
    let migration = migrations.values().next().expect("one migration");
    assert_eq!(migration.description(), "create_users");
    assert!(migration.up_sql().is_empty());
    assert!(migration.down_sql().is_empty());
  }

  #[test]
  fn duplicate_versions_are_rejected() {
    let dir = tempfile::tempdir().expect("temp dir");
    for name in ["0000000000001_a.sql", "0000000000001_b.sql"] {
      std::fs::write(dir.path().join(name), "-- !UP\n\n-- !DOWN\n").expect("write migration");
    }

    let error = build_migration_list(dir.path()).expect_err("duplicate versions must fail");
    assert!(matches!(error, Error::Migration(_)));
    assert!(error.to_string().contains("0000000000001"));
  }

  #[test]
  fn description_strips_suffix() {
    let migration = MigrationFile {
      content_up: None,
      content_down: None,
      number: 1,
      filename: "0000000000001_add_users.sql".to_string(),
    };
    assert_eq!(migration.description(), "add_users");
  }
}
