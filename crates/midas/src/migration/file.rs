//! Migration filename parsing, discovery, and creation.

use std::collections::{
  BTreeMap,
  BTreeSet,
};
use std::fs::{
  self,
  OpenOptions,
};
use std::io::Write as _;
use std::path::{
  Path,
  PathBuf,
};
use std::time::{
  SystemTime,
  UNIX_EPOCH,
};

use super::{
  Migration,
  MigrationFormat,
};
use crate::{
  Error,
  Result,
};

const SINGLE_SUFFIX: &str = ".sql";
const UP_SUFFIX: &str = ".up.sql";
const DOWN_SUFFIX: &str = ".down.sql";

/// Which part of a migration filename was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilePart {
  /// A single `.sql` file.
  Single,
  /// The UP half of a reversible pair.
  Up,
  /// The DOWN half of a reversible pair.
  Down,
}

/// Parsed migration filename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFilename {
  /// Migration version.
  pub version: i64,
  /// Migration description.
  pub description: String,
  /// Expected file format.
  pub format: MigrationFormat,
  /// Which half of a pair this file represents.
  pub part: FilePart,
}

/// Ordered migrations keyed by version.
pub type MigrationList = BTreeMap<i64, Migration>;

/// Parse `<version>_<description>[.up|.down].sql`.
pub fn parse_filename(path: &Path) -> Result<ParsedFilename> {
  let filename = path
    .file_name()
    .and_then(|name| name.to_str())
    .ok_or_else(|| Error::migration(format!("invalid migration filename `{}`", path.to_string_lossy())))?;

  let (stem, part, format) = if let Some(stem) = filename.strip_suffix(UP_SUFFIX) {
    (stem, FilePart::Up, MigrationFormat::SqlxUpDown)
  } else if let Some(stem) = filename.strip_suffix(DOWN_SUFFIX) {
    (stem, FilePart::Down, MigrationFormat::SqlxUpDown)
  } else if let Some(stem) = filename.strip_suffix(SINGLE_SUFFIX) {
    (stem, FilePart::Single, MigrationFormat::Midas)
  } else {
    return Err(Error::migration(format!(
      "invalid migration filename `{filename}`"
    )));
  };

  let (version, description) = stem
    .split_once('_')
    .ok_or_else(|| Error::migration(format!("invalid migration filename `{filename}`")))?;

  let version = version
    .parse::<i64>()
    .map_err(|error| Error::migration(format!("invalid migration version in `{filename}`: {error}")))?;
  if version <= 0 {
    return Err(Error::migration(format!(
      "migration version must be greater than zero in `{filename}`"
    )));
  }

  if description.is_empty()
    || !description
      .chars()
      .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
  {
    return Err(Error::migration(format!(
      "invalid migration description in `{filename}`; expected [A-Za-z0-9._-]+"
    )));
  }

  Ok(ParsedFilename {
    version,
    description: description.to_string(),
    format,
    part,
  })
}

/// Build the migration list for a directory. Missing directories are empty.
pub fn discover(path: &Path) -> Result<MigrationList> {
  if !path.exists() {
    return Ok(MigrationList::new());
  }

  let mut singles = BTreeMap::new();
  let mut ups: BTreeMap<i64, (ParsedFilename, PathBuf)> = BTreeMap::new();
  let mut downs: BTreeMap<i64, (ParsedFilename, PathBuf)> = BTreeMap::new();

  for entry in fs::read_dir(path)? {
    let entry = entry?;
    if !entry.path().is_file() {
      continue;
    }
    if entry.path().extension().and_then(|ext| ext.to_str()) != Some("sql") {
      continue;
    }

    let parsed = parse_filename(&entry.path())?;
    let version = parsed.version;
    match parsed.part {
      FilePart::Single => {
        let migration = Migration::parse_bytes(&entry.path(), &fs::read(entry.path())?)?;
        if singles.insert(version, migration).is_some() {
          return Err(duplicate(version));
        }
      },
      FilePart::Up => {
        if ups.insert(version, (parsed, entry.path())).is_some() {
          return Err(duplicate(version));
        }
      },
      FilePart::Down => {
        if downs.insert(version, (parsed, entry.path())).is_some() {
          return Err(duplicate(version));
        }
      },
    }
  }

  let mut migrations = MigrationList::new();
  for (version, migration) in singles {
    if ups.contains_key(&version) || downs.contains_key(&version) {
      return Err(duplicate(version));
    }
    migrations.insert(version, migration);
  }

  for (version, (parsed, up_path)) in ups {
    let (down_parsed, down_path) = downs.remove(&version).unzip();
    if let Some(down_parsed) = &down_parsed {
      if down_parsed.description != parsed.description {
        return Err(Error::migration(format!(
          "mismatched descriptions for migration {version:013}: `{}` and `{}`",
          parsed.description, down_parsed.description
        )));
      }
    }

    let up = fs::read_to_string(&up_path)?;
    let down = match down_path {
      Some(down_path) => Some(fs::read_to_string(down_path)?),
      None => None,
    };
    migrations.insert(
      version,
      Migration::from_parts(
        version,
        parsed.description,
        up,
        down,
        detect_no_transaction(&fs::read_to_string(&up_path)?),
        up_path,
        MigrationFormat::SqlxUpDown,
      ),
    );
  }

  if let Some((version, _)) = downs.into_iter().next() {
    return Err(Error::migration(format!(
      "migration {version:013} has a DOWN file without an UP file"
    )));
  }

  Ok(migrations)
}

fn detect_no_transaction(content: &str) -> bool {
  content
    .lines()
    .map(str::trim)
    .find(|line| !line.is_empty())
    .is_some_and(|line| line == "-- no-transaction")
}

fn duplicate(version: i64) -> Error {
  Error::migration(format!("duplicate migration version {version:013}"))
}

/// Create a new migration file.
///
/// Returns the version assigned to the new migration.
pub fn create_migration_file(path: &Path, slug: &str, reversible: bool) -> Result<i64> {
  let slug = normalize_slug(slug)?;
  fs::create_dir_all(path)?;

  let version = next_version(path)?;
  if reversible {
    let up = path.join(format!("{version:013}_{slug}.up.sql"));
    let down = path.join(format!("{version:013}_{slug}.down.sql"));
    write_new(&up, "-- Write your UP migration here.\n")?;
    if let Err(error) = write_new(&down, "-- Write your DOWN migration here.\n") {
      let _ = fs::remove_file(&up);
      return Err(error);
    }
  } else {
    let file = path.join(format!("{version:013}_{slug}.sql"));
    write_new(
      &file,
      "-- Put your SQL below the migration separator.\n-- !UP\n\n-- !DOWN\n",
    )?;
  }

  Ok(version)
}

fn normalize_slug(slug: &str) -> Result<String> {
  let slug = slug.trim().to_ascii_lowercase().replace(char::is_whitespace, "_");
  if slug.is_empty()
    || slug.contains("..")
    || slug.contains('/')
    || slug.contains('\\')
    || !slug
      .chars()
      .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
  {
    return Err(Error::migration(format!(
      "invalid migration name `{slug}`; expected [a-z0-9._-]"
    )));
  }
  Ok(slug)
}

fn next_version(path: &Path) -> Result<i64> {
  let existing = existing_versions(path)?;
  let mut version = timestamp_millis()?;
  while existing.contains(&version) {
    version = version
      .checked_add(1)
      .ok_or_else(|| Error::migration("migration version overflow"))?;
  }
  Ok(version)
}

fn existing_versions(path: &Path) -> Result<BTreeSet<i64>> {
  let mut versions = BTreeSet::new();
  if !path.exists() {
    return Ok(versions);
  }
  for entry in fs::read_dir(path)? {
    let entry = entry?;
    if !entry.path().is_file() {
      continue;
    }
    if entry.path().extension().and_then(|ext| ext.to_str()) != Some("sql") {
      continue;
    }
    if let Ok(parsed) = parse_filename(&entry.path()) {
      versions.insert(parsed.version);
    }
  }
  Ok(versions)
}

fn timestamp_millis() -> Result<i64> {
  let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_err(|_| Error::migration("system clock is before the Unix epoch"))?;
  i64::try_from(now.as_millis()).map_err(|_| Error::migration("system clock is out of range"))
}

fn write_new(path: &Path, contents: &str) -> Result<()> {
  let mut file = OpenOptions::new()
    .write(true)
    .create_new(true)
    .open(path)
    .map_err(|error| {
      if error.kind() == std::io::ErrorKind::AlreadyExists {
        Error::migration(format!(
          "migration file `{}` already exists",
          path.to_string_lossy()
        ))
      } else {
        Error::Io(error)
      }
    })?;
  file.write_all(contents.as_bytes())?;
  file.sync_all()?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn version_and_description_are_parsed() {
    let cases = [
      ("1_init.sql", 1, "init", FilePart::Single),
      (
        "20240101000000_init.sql",
        20240101000000,
        "init",
        FilePart::Single,
      ),
      ("1_add-users.sql", 1, "add-users", FilePart::Single),
      ("1_add.users.sql", 1, "add.users", FilePart::Single),
      ("1_init.up.sql", 1, "init", FilePart::Up),
      ("1_init.down.sql", 1, "init", FilePart::Down),
    ];

    for (filename, version, description, part) in cases {
      let parsed = parse_filename(Path::new(filename)).expect("valid filename");
      assert_eq!(parsed.version, version);
      assert_eq!(parsed.description, description);
      assert_eq!(parsed.part, part);
    }
  }

  #[test]
  fn invalid_filenames_are_rejected() {
    for filename in [
      "0_init.sql",
      "-1_init.sql",
      "schema.sql",
      "1_.sql",
      "1_bad name.sql",
    ] {
      assert!(
        parse_filename(Path::new(filename)).is_err(),
        "`{filename}` must be rejected"
      );
    }
  }

  #[test]
  fn discovery_reads_single_and_paired_files() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
      dir.path().join("1_init.sql"),
      "-- !UP\ncreate table a (id int);\n-- !DOWN\ndrop table a;\n",
    )
    .expect("write");
    fs::write(dir.path().join("2_pair.up.sql"), "create table b (id int);\n").expect("write");
    fs::write(dir.path().join("2_pair.down.sql"), "drop table b;\n").expect("write");

    let migrations = discover(dir.path()).expect("discover");
    assert_eq!(migrations.len(), 2);
    assert_eq!(migrations[&1].up, "create table a (id int);\n");
    assert_eq!(migrations[&2].up, "create table b (id int);\n");
    assert_eq!(migrations[&2].down.as_deref(), Some("drop table b;\n"));
    assert_eq!(migrations[&2].format, MigrationFormat::SqlxUpDown);
  }

  #[test]
  fn duplicate_versions_are_rejected() {
    let dir = tempfile::tempdir().expect("temp dir");
    for name in ["1_a.sql", "1_b.sql"] {
      fs::write(dir.path().join(name), "-- !UP\nselect 1;\n").expect("write");
    }
    let error = discover(dir.path()).expect_err("duplicate versions must fail");
    assert!(error.to_string().contains("duplicate migration version"));
  }

  #[test]
  fn down_without_up_is_rejected() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(dir.path().join("1_init.down.sql"), "drop table a;\n").expect("write");
    assert!(discover(dir.path()).is_err());
  }

  #[test]
  fn creation_writes_canonical_files() {
    let dir = tempfile::tempdir().expect("temp dir");
    let version = create_migration_file(dir.path(), "Add Users Table", false).expect("create");
    let path = dir.path().join(format!("{version:013}_add_users_table.sql"));
    let contents = fs::read_to_string(path).expect("read");
    assert!(contents.contains("-- !UP"));
    assert!(contents.contains("-- !DOWN"));
  }

  #[test]
  fn reversible_creation_writes_a_pair() {
    let dir = tempfile::tempdir().expect("temp dir");
    let version = create_migration_file(dir.path(), "init", true).expect("create");
    assert!(dir.path().join(format!("{version:013}_init.up.sql")).is_file());
    assert!(dir.path().join(format!("{version:013}_init.down.sql")).is_file());
  }

  #[test]
  fn unsafe_slugs_are_rejected() {
    for slug in ["../../etc/passwd", "a/b", "a\\b", ""] {
      assert!(normalize_slug(slug).is_err(), "`{slug}` must be rejected");
    }
  }

  #[test]
  fn collisions_increment_the_version() {
    let dir = tempfile::tempdir().expect("temp dir");
    let first = create_migration_file(dir.path(), "first", false).expect("create");
    let second = create_migration_file(dir.path(), "second", false).expect("create");
    assert!(second > first);
  }
}
