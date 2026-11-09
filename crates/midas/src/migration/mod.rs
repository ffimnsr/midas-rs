//! Raw migration model, file discovery, metadata records, and locking.
//!
//! The migration engine preserves the raw UP/DOWN SQL exactly as written.
//! Line endings are normalized only when computing checksums, never for
//! execution.

pub mod file;
pub mod lock;
pub mod record;

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::path::{
  Path,
  PathBuf,
};

pub use file::{
  create_migration_file,
  create_migration_file_with,
  discover,
  discover_with,
  parse_filename,
  validate_description,
  FilePart,
  MigrationList,
  ParsedFilename,
  SqlFilePolicy,
  Versioning,
};
pub use lock::LockGuard;
pub use record::{
  checksum_hex,
  checksum_is_unverified,
  MigrationRecord,
  CHECKSUM_LEN,
};

use sha2::{
  Digest,
  Sha384,
};

use crate::{
  Error,
  Result,
};

const UP_MARKER: &str = "-- !UP";
const DOWN_MARKER: &str = "-- !DOWN";
const NO_TRANSACTION: &str = "-- no-transaction";

/// How a migration file stores its SQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationFormat {
  /// One `.sql` file with `-- !UP` and an optional `-- !DOWN` section.
  Midas,
  /// A `.up.sql` / `.down.sql` pair, SQLx reversible style.
  SqlxUpDown,
  /// A single `.sql` file that only contains UP SQL, SQLx one-way style.
  SqlxOneWay,
}

/// One migration ready to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
  /// Migration version.
  pub version: i64,
  /// Human-readable description.
  pub description: String,
  /// Raw UP SQL, exactly as written.
  pub up: String,
  /// Raw DOWN SQL, exactly as written.
  pub down: Option<String>,
  /// Whether the migration opts out of a transaction.
  pub no_tx: bool,
  /// SHA-384 checksum of the normalized UP SQL.
  pub checksum_up: [u8; CHECKSUM_LEN],
  /// SHA-384 checksum of the normalized DOWN SQL.
  pub checksum_down: Option<[u8; CHECKSUM_LEN]>,
  /// Path the migration was read from.
  pub source_path: PathBuf,
  /// File format.
  pub format: MigrationFormat,
}

impl Migration {
  /// Build a migration from already-separated parts.
  pub fn from_parts(
    version: i64,
    description: impl Into<String>,
    up: impl Into<String>,
    down: Option<String>,
    no_tx: bool,
    source_path: impl Into<PathBuf>,
    format: MigrationFormat,
  ) -> Self {
    let up = up.into();
    Self {
      version,
      description: description.into(),
      checksum_up: checksum(&up),
      checksum_down: down.as_deref().map(checksum),
      up,
      down,
      no_tx,
      source_path: source_path.into(),
      format,
    }
  }

  /// Parse a migration from raw file bytes.
  pub fn parse_bytes(path: &Path, bytes: &[u8]) -> Result<Self> {
    let bytes = strip_bom(bytes);
    let content = std::str::from_utf8(bytes).map_err(|error| {
      Error::migration(format!(
        "invalid UTF-8 in `{}` at byte {}",
        path.to_string_lossy(),
        error.valid_up_to()
      ))
    })?;

    let parsed = parse_filename(path)?;
    let sections = split_sections(content);

    let (format, up, down) = match parsed.part {
      FilePart::Up => (MigrationFormat::SqlxUpDown, content.to_string(), None),
      FilePart::Down => (
        MigrationFormat::SqlxUpDown,
        String::new(),
        Some(content.to_string()),
      ),
      FilePart::Single => match sections.up {
        Some(up) => (
          MigrationFormat::Midas,
          up.to_string(),
          sections.down.map(ToString::to_string),
        ),
        None => (MigrationFormat::SqlxOneWay, content.to_string(), None),
      },
    };

    let no_tx = detect_no_transaction(content, sections.up.is_some());

    Ok(Self::from_parts(
      parsed.version,
      parsed.description,
      up,
      down,
      no_tx,
      path.to_path_buf(),
      format,
    ))
  }

  /// UP checksum as lowercase hexadecimal.
  pub fn checksum_up_hex(&self) -> String {
    checksum_hex(&self.checksum_up)
  }

  /// DOWN checksum as lowercase hexadecimal.
  pub fn checksum_down_hex(&self) -> Option<String> {
    self.checksum_down.as_ref().map(|checksum| checksum_hex(checksum))
  }

  /// Compute UP and DOWN checksums using a specific checksum contract.
  pub fn checksums_for(&self, mode: ChecksumMode) -> ([u8; CHECKSUM_LEN], Option<[u8; CHECKSUM_LEN]>) {
    (
      mode.checksum(&self.up),
      self.down.as_deref().map(|down| mode.checksum(down)),
    )
  }
}

/// Checksum contract used by a metadata table.
///
/// Midas normalizes line endings before hashing. SQLx hashes the raw file
/// content and optionally removes configured characters. Adopted SQLx tables
/// must be validated with the SQLx contract.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ChecksumMode {
  /// Midas checksums with CRLF normalization.
  #[default]
  Midas,
  /// SQLx checksums over raw content with ignored characters removed.
  Sqlx {
    /// Characters removed before hashing.
    ignored_chars: BTreeSet<char>,
  },
}

impl ChecksumMode {
  /// Compute a checksum according to this mode.
  pub fn checksum(&self, sql: &str) -> [u8; CHECKSUM_LEN] {
    match self {
      Self::Midas => checksum(sql),
      Self::Sqlx { ignored_chars } => checksum_sqlx(sql, ignored_chars),
    }
  }
}

/// Compute a SQLx-compatible SHA-384 checksum.
///
pub(crate) fn checksum_sqlx(sql: &str, ignored_chars: &BTreeSet<char>) -> [u8; CHECKSUM_LEN] {
  let mut hasher = Sha384::new();
  if ignored_chars.is_empty() {
    hasher.update(sql.as_bytes());
  } else {
    for fragment in sql.split(|character| ignored_chars.contains(&character)) {
      hasher.update(fragment.as_bytes());
    }
  }
  let digest = hasher.finalize();
  let mut out = [0u8; CHECKSUM_LEN];
  out.copy_from_slice(&digest);
  out
}

/// Compute the SHA-384 checksum of SQL after newline normalization.
pub fn checksum(sql: &str) -> [u8; CHECKSUM_LEN] {
  let normalized = normalize_newlines(sql);
  let digest = Sha384::digest(normalized.as_bytes());
  let mut out = [0u8; CHECKSUM_LEN];
  out.copy_from_slice(&digest);
  out
}

fn strip_bom(bytes: &[u8]) -> &[u8] {
  bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes)
}

fn normalize_newlines(sql: &str) -> Cow<'_, str> {
  if sql.contains('\r') {
    Cow::Owned(sql.replace("\r\n", "\n").replace('\r', "\n"))
  } else {
    Cow::Borrowed(sql)
  }
}

struct Sections<'a> {
  up: Option<&'a str>,
  down: Option<&'a str>,
}

/// Split content into UP and DOWN sections using exact marker lines.
fn find_marker(content: &str, marker: &str) -> Option<(usize, usize)> {
  let mut offset = 0;
  for line in content.split_inclusive('\n') {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    if trimmed == marker {
      return Some((offset, offset + line.len()));
    }
    offset += line.len();
  }
  None
}

fn split_sections(content: &str) -> Sections<'_> {
  let Some((_, up_end)) = find_marker(content, UP_MARKER) else {
    return Sections { up: None, down: None };
  };

  let down_marker =
    find_marker(&content[up_end..], DOWN_MARKER).map(|(start, end)| (up_end + start, up_end + end));
  match down_marker {
    Some((down_start, down_end)) => Sections {
      up: Some(&content[up_end..down_start]),
      down: Some(&content[down_end..]),
    },
    None => Sections {
      up: Some(&content[up_end..]),
      down: None,
    },
  }
}

fn detect_no_transaction(content: &str, has_up_marker: bool) -> bool {
  let mut lines = content.lines();
  if has_up_marker {
    // Before the UP marker.
    for line in lines.by_ref() {
      let trimmed = line.trim();
      if trimmed == UP_MARKER {
        break;
      }
      if trimmed.is_empty() || trimmed.starts_with("-- #") {
        continue;
      }
      if trimmed == NO_TRANSACTION {
        return true;
      }
      break;
    }
    // Immediately after the UP marker.
    for line in lines {
      let trimmed = line.trim();
      if trimmed.is_empty() {
        continue;
      }
      return trimmed == NO_TRANSACTION;
    }
    false
  } else {
    for line in lines {
      let trimmed = line.trim();
      if trimmed.is_empty() {
        continue;
      }
      return trimmed == NO_TRANSACTION;
    }
    false
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn parse(name: &str, content: &str) -> Migration {
    Migration::parse_bytes(Path::new(name), content.as_bytes()).expect("parse migration")
  }

  #[test]
  fn blank_lines_are_preserved() {
    let migration = parse(
      "1_init.sql",
      "-- !UP\ncreate table a (id int);\n\ncreate table b (id int);\n-- !DOWN\ndrop table b;\ndrop table a;\n",
    );
    assert_eq!(
      migration.up,
      "create table a (id int);\n\ncreate table b (id int);\n"
    );
    assert_eq!(migration.down.as_deref(), Some("drop table b;\ndrop table a;\n"));
  }

  #[test]
  fn dollar_quoted_bodies_are_byte_identical() {
    let body = "-- !UP\ncreate function f() returns void as $$\nbegin\n  perform 1; -- keep\nend;\n$$ language plpgsql;\n-- !DOWN\ndrop function f();\n";
    let migration = parse("1_fn.sql", body);
    assert!(migration.up.contains("$$ language plpgsql;\n"));
    assert!(migration.up.contains("  perform 1; -- keep\n"));
  }

  #[test]
  fn crlf_and_lf_checksums_match() {
    let lf = parse("1_init.sql", "-- !UP\nselect 1;\n-- !DOWN\n");
    let crlf = parse("1_init.sql", "-- !UP\r\nselect 1;\r\n-- !DOWN\r\n");
    assert_eq!(lf.checksum_up, crlf.checksum_up);
    assert_eq!(lf.up, "select 1;\n");
    assert_eq!(crlf.up, "select 1;\r\n");
  }

  #[test]
  fn bom_is_stripped() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"-- !UP\nselect 1;\n-- !DOWN\n");
    let migration = Migration::parse_bytes(Path::new("1_init.sql"), &bytes).expect("parse");
    assert_eq!(migration.checksum_up, checksum("select 1;\n"));
  }

  #[test]
  fn missing_down_is_allowed() {
    let migration = parse("1_init.sql", "-- !UP\nselect 1;\n");
    assert!(migration.down.is_none());
    assert!(migration.checksum_down.is_none());
  }

  #[test]
  fn no_transaction_is_detected() {
    let after_up = parse("1_init.sql", "-- !UP\n-- no-transaction\nselect 1;\n");
    assert!(after_up.no_tx);

    let before_up = parse("1_init.sql", "-- no-transaction\n-- !UP\nselect 1;\n");
    assert!(before_up.no_tx);

    let one_way = parse("1_init.sql", "-- no-transaction\nselect 1;\n");
    assert!(one_way.no_tx);
    assert_eq!(one_way.format, MigrationFormat::SqlxOneWay);
  }

  #[test]
  fn invalid_utf8_reports_the_path() {
    let error = Migration::parse_bytes(Path::new("1_bad.sql"), &[0xFF, 0xFE]).expect_err("invalid utf8");
    assert!(error.to_string().contains("1_bad.sql"));
    assert!(error.to_string().contains("byte 0"));
  }

  #[test]
  fn one_way_files_use_the_whole_body() {
    let migration = parse("1_init.sql", "select 1;\nselect 2;\n");
    assert_eq!(migration.up, "select 1;\nselect 2;\n");
    assert!(migration.down.is_none());
    assert_eq!(migration.format, MigrationFormat::SqlxOneWay);
  }

  #[test]
  fn checksum_is_stable_hex() {
    let migration = parse("1_init.sql", "-- !UP\nselect 1;\n-- !DOWN\n");
    assert_eq!(migration.checksum_up_hex().len(), CHECKSUM_LEN * 2);
  }
}
