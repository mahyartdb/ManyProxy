//! Preset UI: rows, fold sections, subscription and rule-set rendering. The
//! preset data shapes and matching live in `manyproxy_core::preset`.

pub mod rules;
pub mod sets;

// The core preset shapes re-export publicly: the split test binaries reach
// them as `manyproxy_ui::preset::…`.
pub use manyproxy_core::preset::*;
pub use rules::*;
pub use sets::*;
