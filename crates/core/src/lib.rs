//! ManyProxy's pure logic: store, engine lifecycle, config import, health
//! probes, presets, update checks.
//! Belongs: anything that runs without a window.
//! Not here: gpui or anything that needs one — `cargo tree -p manyproxy-core`
//! is the proof that this crate never pulls the UI stack.

pub mod consts;
pub mod dpapi;
pub mod engine;
pub mod health;
pub mod import;
pub mod import_flow;
pub mod library;
pub mod models;
pub mod preset;
pub mod secret;
pub mod single_instance;
pub mod store;
pub mod text;
pub mod update;

mod process;

pub use consts::*;
pub use dpapi::*;
pub use engine::*;
pub use health::*;
pub use import::*;
pub use import_flow::*;
pub use library::*;
pub use models::*;
pub use preset::*;
pub use process::*;
pub use secret::*;
pub use single_instance::*;
pub use store::*;
pub use text::*;
pub use update::*;
