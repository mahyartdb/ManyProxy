//! Preset UI: rows, fold sections, subscription and rule-set rendering. The
//! preset data shapes and matching live in `manyproxy_core::preset`.

pub mod rules;
pub mod sets;

// Crate-visible, not `pub`: a pub glob would re-export core's
// `group_quota_line` next to the UI's own and make the root glob ambiguous.
pub(crate) use manyproxy_core::preset::*;
pub use rules::*;
pub use sets::*;
