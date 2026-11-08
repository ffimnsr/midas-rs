//! SQL dialect capabilities.

/// A capability that differs between database engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Feature {
  /// Named schemas inside a database.
  Schemas,
  /// DDL that participates in transactions.
  TransactionalDdl,
  /// Session-level advisory locks.
  AdvisoryLocks,
  /// `ALTER COLUMN ... TYPE` support.
  AlterColumnType,
  /// Sending several statements in one batch.
  MultiStatementBatch,
  /// `RETURNING` clauses.
  ReturningClause,
}

/// A literal value ready to be emitted as SQL.
#[derive(Debug, Clone, PartialEq)]
pub enum SqlLiteral<'a> {
  /// SQL `NULL`.
  Null,
  /// Boolean literal.
  Bool(bool),
  /// Integer literal.
  Integer(i64),
  /// Floating-point literal.
  Float(f64),
  /// String literal.
  Text(&'a str),
}

/// Quoting and capability rules for one database engine.
pub trait Dialect {
  /// Quote an identifier.
  fn quote_ident(&self, ident: &str) -> String;

  /// Quote a string as a SQL literal.
  fn quote_literal(&self, value: &str) -> String;

  /// Emit a typed literal.
  fn emit_literal(&self, value: &SqlLiteral<'_>) -> String;

  /// Report whether the dialect supports a feature.
  fn supports(&self, feature: Feature) -> bool;
}

pub(crate) fn quote_ident_with(ident: &str, quote: char) -> String {
  let escaped = ident.replace(quote, &format!("{quote}{quote}"));
  format!("{quote}{escaped}{quote}")
}

pub(crate) fn quote_literal_with(value: &str, quote: char) -> String {
  let escaped = value.replace(quote, &format!("{quote}{quote}"));
  format!("{quote}{escaped}{quote}")
}

pub(crate) fn emit_literal_with(value: &SqlLiteral<'_>, quote: char) -> String {
  match value {
    SqlLiteral::Null => "NULL".to_string(),
    SqlLiteral::Bool(value) => {
      if *value {
        "TRUE".to_string()
      } else {
        "FALSE".to_string()
      }
    },
    SqlLiteral::Integer(value) => value.to_string(),
    SqlLiteral::Float(value) => value.to_string(),
    SqlLiteral::Text(value) => quote_literal_with(value, quote),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn identifier_quotes_are_doubled() {
    assert_eq!(quote_ident_with("we\"ird", '"'), "\"we\"\"ird\"");
    assert_eq!(quote_ident_with("we`ird", '`'), "`we``ird`");
  }

  #[test]
  fn literal_quotes_are_doubled() {
    assert_eq!(quote_literal_with("it's", '\''), "'it''s'");
  }

  #[test]
  fn literals_render() {
    assert_eq!(emit_literal_with(&SqlLiteral::Null, '\''), "NULL");
    assert_eq!(emit_literal_with(&SqlLiteral::Bool(true), '\''), "TRUE");
    assert_eq!(emit_literal_with(&SqlLiteral::Integer(7), '\''), "7");
    assert_eq!(emit_literal_with(&SqlLiteral::Text("a'b"), '\''), "'a''b'");
  }
}
