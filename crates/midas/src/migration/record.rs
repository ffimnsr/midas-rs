//! SQLx-compatible migration metadata records.

/// Length of a SHA-384 checksum in bytes.
pub const CHECKSUM_LEN: usize = 48;

/// A row in the schema migrations metadata table.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MigrationRecord {
  /// Migration version.
  pub version: i64,
  /// Human-readable migration description.
  pub description: String,
  /// Installation timestamp rendered by the database.
  pub installed_on: String,
  /// Whether the migration completed successfully.
  pub success: bool,
  /// SHA-384 checksum over the normalized UP SQL.
  pub checksum: Vec<u8>,
  /// Execution time in nanoseconds.
  pub execution_time: i64,
  /// SHA-384 checksum over the normalized DOWN SQL.
  pub down_checksum: Option<Vec<u8>>,
}

impl MigrationRecord {
  /// Build a successfully applied record.
  pub fn applied(
    version: i64,
    description: impl Into<String>,
    checksum: impl Into<Vec<u8>>,
    down_checksum: Option<Vec<u8>>,
    execution_time: i64,
  ) -> Self {
    Self {
      version,
      description: description.into(),
      installed_on: String::new(),
      success: true,
      checksum: checksum.into(),
      execution_time,
      down_checksum,
    }
  }

  /// Whether the stored checksum is a legacy placeholder that cannot be
  /// compared with a file checksum.
  pub fn is_unverified(&self) -> bool {
    checksum_is_unverified(&self.checksum)
  }

  /// Checksum as lowercase hexadecimal.
  pub fn checksum_hex(&self) -> String {
    checksum_hex(&self.checksum)
  }
}

/// All-zero or empty checksums come from legacy metadata rows.
pub fn checksum_is_unverified(checksum: &[u8]) -> bool {
  checksum.is_empty() || checksum.iter().all(|byte| *byte == 0)
}

/// Render bytes as lowercase hexadecimal.
pub fn checksum_hex(bytes: &[u8]) -> String {
  const HEX: &[u8; 16] = b"0123456789abcdef";
  let mut out = String::with_capacity(bytes.len() * 2);
  for byte in bytes {
    out.push(HEX[(byte >> 4) as usize] as char);
    out.push(HEX[(byte & 0x0f) as usize] as char);
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn legacy_checksums_are_detected() {
    assert!(checksum_is_unverified(&[]));
    assert!(checksum_is_unverified(&[0; CHECKSUM_LEN]));
    assert!(!checksum_is_unverified(&[1; CHECKSUM_LEN]));
  }

  #[test]
  fn hex_rendering_is_lowercase() {
    assert_eq!(checksum_hex(&[0x0a, 0xff]), "0aff");
  }

  #[test]
  fn applied_records_are_successful() {
    let record = MigrationRecord::applied(1, "init", vec![7; CHECKSUM_LEN], None, 42);
    assert!(record.success);
    assert_eq!(record.version, 1);
    assert_eq!(record.execution_time, 42);
    assert!(!record.is_unverified());
  }
}
