//! Advisory lock guard.

use std::ops::{
  Deref,
  DerefMut,
};

use crate::sequel::Backend;
use crate::Result;

/// Holds the migrator lock and releases it on drop.
pub struct LockGuard<'a> {
  backend: &'a mut dyn Backend,
  armed: bool,
}

impl<'a> LockGuard<'a> {
  /// Acquire the advisory lock for the backend.
  pub fn acquire(backend: &'a mut dyn Backend) -> Result<Self> {
    backend.lock()?;
    Ok(Self { backend, armed: true })
  }

  /// Release the lock early. Dropping the guard after this is a no-op.
  pub fn unlock(mut self) -> Result<()> {
    self.armed = false;
    self.backend.unlock()
  }
}

impl<'a> Deref for LockGuard<'a> {
  type Target = dyn Backend + 'a;

  fn deref(&self) -> &Self::Target {
    self.backend
  }
}

impl<'a> DerefMut for LockGuard<'a> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    self.backend
  }
}

impl Drop for LockGuard<'_> {
  fn drop(&mut self) {
    if self.armed {
      let _ = self.backend.unlock();
    }
  }
}

#[cfg(test)]
mod tests {
  use std::panic::{
    catch_unwind,
    AssertUnwindSafe,
  };

  use super::*;
  use crate::sequel::memory::MemoryBackend;

  #[test]
  fn guard_unlocks_on_drop() {
    let mut backend = MemoryBackend::new();
    {
      let _guard = LockGuard::acquire(&mut backend).expect("lock");
    }
    assert!(!backend.is_locked());
  }

  #[test]
  fn guard_unlocks_after_panic() {
    let mut backend = MemoryBackend::new();
    let result = catch_unwind(AssertUnwindSafe(|| {
      let _guard = LockGuard::acquire(&mut backend).expect("lock");
      panic!("boom");
    }));
    assert!(result.is_err());
    assert!(!backend.is_locked());
  }

  #[test]
  fn explicit_unlock_is_idempotent() {
    let mut backend = MemoryBackend::new();
    let guard = LockGuard::acquire(&mut backend).expect("lock");
    guard.unlock().expect("unlock");
    assert!(!backend.is_locked());
  }
}
