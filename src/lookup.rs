use anyhow::{
  Context,
  Result as AnyhowResult,
};
use indoc::indoc;
use regex::Regex;
use std::collections::BTreeMap;
use std::fs::{
  read_dir,
  File,
};
use std::io::prelude::*;
use std::io::BufReader;
use std::path::Path;
use std::time::{
  SystemTime,
  UNIX_EPOCH,
};

pub type VecStr = Vec<String>;

#[derive(Debug)]
pub struct MigrationFile {
  pub content_up: Option<VecStr>,
  pub content_down: Option<VecStr>,
  pub number: i64,
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
}

/// A map of migration files
pub type MigrationFiles = BTreeMap<i64, MigrationFile>;

fn parse_file(filename: &str) -> AnyhowResult<MigrationFile> {
  let re = Regex::new(r"^(?P<number>[0-9]{13})_(?P<name>[_0-9a-zA-Z]*)\.sql$")?;

  let result = re
    .captures(filename)
    .with_context(|| format!("Invalid filename found on {filename}"))?;

  let number = result
    .name("number")
    .context("The migration file timestamp is missing")?
    .as_str()
    .parse::<i64>()?;

  Ok(MigrationFile::new(filename, number))
}

pub fn build_migration_list(path: &Path) -> AnyhowResult<MigrationFiles> {
  let mut files: MigrationFiles = BTreeMap::new();

  for entry in read_dir(path)? {
    let entry = entry?;
    let filename = entry.file_name();
    let Ok(info) = parse_file(filename.to_str().context("Filename is not valid")?) else {
      continue;
    };

    let file = File::open(entry.path())?;
    let mut buf_reader = BufReader::new(file);
    let mut content = String::new();
    buf_reader.read_to_string(&mut content)?;

    let split_vec: Vec<String> = content
      .split('\n')
      .map(std::string::ToString::to_string)
      .collect();

    let pos_up = split_vec
      .iter()
      .position(|s| s == "-- !UP" || s == "-- !UP\r")
      .context("Parser can't find the UP migration")?;
    let pos_down = split_vec
      .iter()
      .position(|s| s == "-- !DOWN" || s == "-- !DOWN\r")
      .context("Parser can't find the DOWN migration")?;

    let content_up = &split_vec[(pos_up + 1)..pos_down];
    let content_down = &split_vec[(pos_down + 1)..];

    let migration = MigrationFile {
      content_up: Some(content_up.to_vec()),
      content_down: Some(content_down.to_vec()),
      ..info
    };

    log::trace!("Running the migration: {:?} {:?}", migration, migration.filename);
    files.insert(migration.number, migration);
  }

  Ok(files)
}

/// Generate a timestamp string
fn timestamp() -> String {
  let start = SystemTime::now();
  let since_the_epoch = start.duration_since(UNIX_EPOCH).expect("Time went backwards");
  since_the_epoch.as_millis().to_string()
}

pub fn create_migration_file(path: &Path, slug: &str) -> AnyhowResult<()> {
  let filename = timestamp() + "_" + slug + ".sql";
  let filepath = path.join(filename);

  log::trace!("Creating new migration file: {:?}", filepath);
  let mut f = File::create(filepath)?;
  let contents = indoc! {"\
    -- # Put the your SQL below migration seperator.
    -- !UP

    -- !DOWN
  "};

  f.write_all(contents.as_bytes())?;
  f.sync_all()?;

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parse_file() {
    let result = parse_file("0000000000000_initial.sql").unwrap();
    assert_eq!(result.number, 0);
    assert_eq!(result.filename, "0000000000000_initial.sql");
  }
}
