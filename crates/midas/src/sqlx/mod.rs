//! SQLx interoperability.
//!
//! Midas does not depend on SQLx at runtime. These modules implement the file
//! format, metadata table, and `sqlx.toml` compatibility rules directly.

pub mod adopt;
pub mod config;
pub mod export;
pub mod import;

pub use adopt::{
  baseline_from_sqlx,
  config_for_table,
  detect,
  detect_table,
  verify_adoption,
  AdoptionReport,
  BaselineReport,
};
pub use config::{
  qualified_table_name,
  split_qualified_table,
  SqlxConfig,
  SqlxMigrationType,
  SqlxVersioning,
  UnrecognizedSqlFiles,
  SQLX_CONFIG_FILE,
};
pub use export::export_sqlx;
pub use import::{
  import_sqlx,
  import_sqlx_with,
  write_midas,
};
