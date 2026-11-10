//! VoiceBro application core.
//!
//! Everything the app does lives here behind types and traits; `apps/desktop` only wires
//! these to Tauri commands and events. This crate has no Tauri dependency, so its tests run
//! without a window. See `docs/ARCHITECTURE.md`.

pub mod events;
pub mod paths;
pub mod runtime;
pub mod secrets;
pub mod settings;
pub mod status;
pub mod wake;

pub use events::{AppEvent, EventBus};
pub use runtime::Runtime;
pub use secrets::{SecretKey, SecretStore};
pub use settings::Settings;
pub use status::Status;
