//! Compile fixture for `midas::migrate!` and the SQLite backend.

use midas::commander::Migrator;
use midas::config::Config;
use midas::sequel;

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let migrations = midas::migrate!();
  assert_eq!(migrations.len(), 1);

  let config = Config {
    database_url: Some(":memory:".to_string()),
    ..Config::default()
  };
  let backend = sequel::connect(&config)?;
  let list = migrations
    .into_iter()
    .map(|migration| (migration.version, migration))
    .collect();
  let mut migrator = Migrator::new(backend, list);
  migrator.up(None)?;
  println!("embedded-ok:{}", migrator.info()?.applied.len());
  Ok(())
}
