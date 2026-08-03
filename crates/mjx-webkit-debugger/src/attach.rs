//! Discovery and attachment, shared by the CLI and the picker.
//!
//! **Owned by `docs/tasks/T-010-app-shell.md`.**

use anyhow::Result;

/// Print the inspectable targets at an address.
///
/// Useful on its own — "is my app exposing an inspector at all?" is the first
/// question when an attach fails, and answering it should not require opening
/// a window.
pub fn list(_address: &str) -> Result<()> {
    todo!("T-010")
}
