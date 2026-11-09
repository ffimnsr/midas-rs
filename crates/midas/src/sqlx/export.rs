//! Export Midas migrations as SQLx-compatible files.

use std::fs;
use std::path::{
  Path,
  PathBuf,
};

use crate::migration::file::validate_description;
use crate::migration::{
  Migration,
  MigrationList,
};
use crate::Result;

const NO_TRANSACTION: &str = "-- no-transaction";

/// Write SQLx migration files for every migration in `migrations`.
///
/// Each migration becomes `<version>_<description>.up.sql`; a `.down.sql` file
/// is added when the migration has a DOWN section. SQL content is preserved
/// byte-for-byte. The `-- no-transaction` marker is always the first line of an
/// exported file when the migration opts out of a transaction.
pub fn export_sqlx(migrations: &MigrationList, out_dir: &Path) -> Result<Vec<PathBuf>> {
  fs::create_dir_all(out_dir)?;

  let mut written = Vec::new();
  for migration in migrations.values() {
    validate_description(&migration.description)?;
    let base = format!("{:013}_{}", migration.version, migration.description);

    let up_path = out_dir.join(format!("{base}.up.sql"));
    fs::write(&up_path, exported_up(migration))?;
    written.push(up_path);

    if let Some(down) = &migration.down {
      let down_path = out_dir.join(format!("{base}.down.sql"));
      fs::write(&down_path, down)?;
      written.push(down_path);
    }
  }

  Ok(written)
}

fn exported_up(migration: &Migration) -> String {
  if migration.no_tx && !migration.up.starts_with(NO_TRANSACTION) {
    format!("{NO_TRANSACTION}\n{}", migration.up)
  } else {
    migration.up.clone()
  }
}

#[cfg(test)]
mod tests {
  use std::collections::BTreeMap;

  use super::*;
  use crate::migration::MigrationFormat;

  fn migration(version: i64, description: &str, up: &str, down: Option<&str>, no_tx: bool) -> Migration {
    Migration::from_parts(
      version,
      description,
      up,
      down.map(ToString::to_string),
      no_tx,
      format!("{version:013}_{description}.sql"),
      MigrationFormat::Midas,
    )
  }

  fn list(migrations: Vec<Migration>) -> MigrationList {
    migrations
      .into_iter()
      .map(|migration| (migration.version, migration))
      .collect::<BTreeMap<_, _>>()
  }

  #[test]
  fn reversible_migrations_write_both_files() {
    let dir = tempfile::tempdir().expect("temp dir");
    let migrations = list(vec![migration(
      1,
      "init",
      "create table a (id integer);\n",
      Some("drop table a;\n"),
      false,
    )]);

    let written = export_sqlx(&migrations, dir.path()).expect("export");
    assert_eq!(written.len(), 2);
    assert_eq!(
      fs::read_to_string(dir.path().join("0000000000001_init.up.sql")).expect("up"),
      "create table a (id integer);\n"
    );
    assert_eq!(
      fs::read_to_string(dir.path().join("0000000000001_init.down.sql")).expect("down"),
      "drop table a;\n"
    );
  }

  #[test]
  fn one_way_migrations_write_only_up() {
    let dir = tempfile::tempdir().expect("temp dir");
    let migrations = list(vec![migration(1, "init", "select 1;\n", None, false)]);

    export_sqlx(&migrations, dir.path()).expect("export");
    assert!(dir.path().join("0000000000001_init.up.sql").is_file());
    assert!(!dir.path().join("0000000000001_init.down.sql").exists());
  }

  #[test]
  fn no_transaction_is_always_first() {
    let dir = tempfile::tempdir().expect("temp dir");
    let migrations = list(vec![migration(1, "init", "vacuum;\n", None, true)]);

    export_sqlx(&migrations, dir.path()).expect("export");
    let content = fs::read_to_string(dir.path().join("0000000000001_init.up.sql")).expect("up");
    assert!(content.starts_with("-- no-transaction\nvacuum;\n"));
  }

  #[test]
  fn existing_marker_is_not_duplicated() {
    let dir = tempfile::tempdir().expect("temp dir");
    let migrations = list(vec![migration(
      1,
      "init",
      "-- no-transaction\nvacuum;\n",
      None,
      true,
    )]);

    export_sqlx(&migrations, dir.path()).expect("export");
    let content = fs::read_to_string(dir.path().join("0000000000001_init.up.sql")).expect("up");
    assert_eq!(content, "-- no-transaction\nvacuum;\n");
  }

  #[test]
  fn descriptions_that_cannot_be_filenames_are_rejected() {
    let dir = tempfile::tempdir().expect("temp dir");
    let migrations = list(vec![migration(1, "add users", "select 1;\n", None, false)]);
    assert!(export_sqlx(&migrations, dir.path()).is_err());
  }
}
