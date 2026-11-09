//! Import SQLx migration files into the Midas migration model.

use std::fs;
use std::path::{
  Path,
  PathBuf,
};

use crate::migration::{
  discover_with,
  Migration,
  MigrationList,
  SqlFilePolicy,
};
use crate::sqlx::config::SqlxConfig;
use crate::Result;

const UP_MARKER: &str = "-- !UP";
const DOWN_MARKER: &str = "-- !DOWN";

/// Read every migration in a SQLx migration directory.
///
/// `<version>_<description>.sql` files become one-way migrations;
/// `<version>_<description>.up.sql` and `.down.sql` pairs become reversible
/// migrations. SQL content is preserved and checksums are computed from it.
pub fn import_sqlx(dir: &Path) -> Result<Vec<Migration>> {
  from_list(discover_with(dir, SqlFilePolicy::Error)?)
}

/// Read a SQLx migration directory using settings from `sqlx.toml`.
pub fn import_sqlx_with(dir: &Path, config: &SqlxConfig) -> Result<Vec<Migration>> {
  let policy = match config.unrecognized_sql_files {
    crate::sqlx::config::UnrecognizedSqlFiles::Ignore => SqlFilePolicy::Ignore,
    crate::sqlx::config::UnrecognizedSqlFiles::Warn => SqlFilePolicy::Warn,
    crate::sqlx::config::UnrecognizedSqlFiles::Error => SqlFilePolicy::Error,
  };
  from_list(discover_with(dir, policy)?)
}

/// Write imported migrations back as Midas single-file migrations.
///
/// The UP and DOWN content is preserved; only the `-- !UP` / `-- !DOWN`
/// separators are added so Midas can read the files.
pub fn write_midas(migrations: &[Migration], out_dir: &Path) -> Result<Vec<PathBuf>> {
  fs::create_dir_all(out_dir)?;
  let mut written = Vec::with_capacity(migrations.len());

  for migration in migrations {
    crate::migration::file::validate_description(&migration.description)?;
    let path = out_dir.join(format!("{:013}_{}.sql", migration.version, migration.description));

    let mut content = String::new();
    content.push_str(UP_MARKER);
    content.push('\n');
    content.push_str(&migration.up);
    if !content.ends_with('\n') {
      content.push('\n');
    }

    if let Some(down) = &migration.down {
      content.push_str(DOWN_MARKER);
      content.push('\n');
      content.push_str(down);
      if !content.ends_with('\n') {
        content.push('\n');
      }
    }

    fs::write(&path, content)?;
    written.push(path);
  }

  Ok(written)
}

fn from_list(migrations: MigrationList) -> Result<Vec<Migration>> {
  Ok(migrations.into_values().collect())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::sqlx::export::export_sqlx;

  fn write(dir: &Path, name: &str, content: &str) {
    fs::write(dir.join(name), content).expect("write migration");
  }

  #[test]
  fn one_way_files_have_no_down() {
    let dir = tempfile::tempdir().expect("temp dir");
    write(dir.path(), "1_init.sql", "create table a (id integer);\n");

    let migrations = import_sqlx(dir.path()).expect("import");
    assert_eq!(migrations.len(), 1);
    assert_eq!(migrations[0].version, 1);
    assert_eq!(migrations[0].description, "init");
    assert_eq!(migrations[0].up, "create table a (id integer);\n");
    assert!(migrations[0].down.is_none());
  }

  #[test]
  fn reversible_pairs_are_combined() {
    let dir = tempfile::tempdir().expect("temp dir");
    write(dir.path(), "2_pair.up.sql", "create table b (id integer);\n");
    write(dir.path(), "2_pair.down.sql", "drop table b;\n");

    let migrations = import_sqlx(dir.path()).expect("import");
    assert_eq!(migrations[0].up, "create table b (id integer);\n");
    assert_eq!(migrations[0].down.as_deref(), Some("drop table b;\n"));
  }

  #[test]
  fn no_transaction_is_detected() {
    let dir = tempfile::tempdir().expect("temp dir");
    write(dir.path(), "1_init.sql", "-- no-transaction\nvacuum;\n");

    let migrations = import_sqlx(dir.path()).expect("import");
    assert!(migrations[0].no_tx);
  }

  #[test]
  fn export_import_export_is_stable() {
    let source = tempfile::tempdir().expect("source");
    write(
      source.path(),
      "1_init.sql",
      "-- !UP\ncreate table a (id integer);\n-- !DOWN\ndrop table a;\n",
    );
    write(source.path(), "2_two.sql", "-- !UP\nselect 1;\n");

    let first_export = tempfile::tempdir().expect("first export");
    let migrations = discover_with(source.path(), SqlFilePolicy::Error).expect("discover");
    export_sqlx(&migrations, first_export.path()).expect("export");

    let imported = import_sqlx(first_export.path()).expect("import");
    let imported_list = imported
      .iter()
      .cloned()
      .map(|migration| (migration.version, migration))
      .collect::<MigrationList>();

    let second_export = tempfile::tempdir().expect("second export");
    export_sqlx(&imported_list, second_export.path()).expect("export");

    for name in [
      "0000000000001_init.up.sql",
      "0000000000001_init.down.sql",
      "0000000000002_two.up.sql",
    ] {
      assert_eq!(
        fs::read(first_export.path().join(name)).expect("first"),
        fs::read(second_export.path().join(name)).expect("second"),
        "`{name}` must round-trip byte-for-byte"
      );
    }

    for migration in &imported {
      let stored = first_export.path().join(format!(
        "{:013}_{}.up.sql",
        migration.version, migration.description
      ));
      assert_eq!(
        fs::read_to_string(stored).expect("exported up"),
        migration.up,
        "up content must be preserved"
      );
    }
    assert_eq!(
      imported_list[&1].checksum_up, migrations[&1].checksum_up,
      "checksums must survive a round trip"
    );
  }

  #[test]
  fn imported_migrations_can_be_written_as_midas_files() {
    let source = tempfile::tempdir().expect("source");
    write(source.path(), "1_init.up.sql", "create table a (id integer);\n");
    write(source.path(), "1_init.down.sql", "drop table a;\n");

    let migrations = import_sqlx(source.path()).expect("import");
    let out = tempfile::tempdir().expect("out");
    let written = write_midas(&migrations, out.path()).expect("write");
    assert_eq!(written.len(), 1);

    let content = fs::read_to_string(out.path().join("0000000000001_init.sql")).expect("read");
    assert_eq!(
      content,
      "-- !UP\ncreate table a (id integer);\n-- !DOWN\ndrop table a;\n"
    );
  }

  #[test]
  fn unrecognized_policy_can_skip_files() {
    let dir = tempfile::tempdir().expect("temp dir");
    write(dir.path(), "1_init.sql", "select 1;\n");
    write(dir.path(), "notes.sql", "not a migration\n");

    let config = SqlxConfig {
      unrecognized_sql_files: crate::sqlx::config::UnrecognizedSqlFiles::Ignore,
      ..SqlxConfig::default()
    };
    let migrations = import_sqlx_with(dir.path(), &config).expect("import");
    assert_eq!(migrations.len(), 1);
  }
}
