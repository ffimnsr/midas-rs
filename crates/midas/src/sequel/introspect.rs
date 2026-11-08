//! Schema introspection abstraction populated by later phases.

use crate::Result;

/// Kind of object found while introspecting a schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectKind {
  /// A table.
  Table,
  /// A view.
  View,
  /// A materialized view.
  MaterializedView,
  /// An enum type.
  Enum,
  /// A sequence.
  Sequence,
  /// A function or procedure.
  Function,
  /// An extension.
  Extension,
  /// Any other object.
  Other,
}

/// A single database object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaObject {
  /// Object name.
  pub name: String,
  /// Object kind.
  pub kind: ObjectKind,
}

/// Snapshot of a single schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaSnapshot {
  /// Schema name.
  pub schema: String,
  /// Objects in the schema.
  pub objects: Vec<SchemaObject>,
}

/// Reads schema structure from a database.
pub trait Introspector {
  /// Introspect one schema.
  fn introspect_schema(&mut self, schema: &str) -> Result<SchemaSnapshot>;

  /// List user-visible schemas.
  fn list_schemas(&mut self) -> Result<Vec<String>>;
}
