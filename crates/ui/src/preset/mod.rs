//! Preset UI: rows, fold sections, subscription and rule-set rendering. The
//! preset data shapes and matching live in `manyproxy_core::preset`.

mod rules;
mod sets;

pub use manyproxy_core::preset::*;
pub(crate) use rules::*;
pub(crate) use sets::*;
