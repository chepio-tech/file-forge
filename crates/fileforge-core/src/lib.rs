//! FileForge processing core: pure functions over bytes and paths, with no UI or Tauri dependency.
//!
//! The desktop shell (`src-tauri`) owns dialogs, events and app state; this crate owns everything that can be tested
//! without a window. See `docs/architecture/components.md`.

pub mod file_kind;
pub mod pdf;

pub use file_kind::FileKind;
