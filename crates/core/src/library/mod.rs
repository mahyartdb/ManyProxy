//! Pure row geometry and search data shared by the list and its header.

mod columns;
#[path = "match.rs"]
mod matching;
mod metrics;
mod rows;

pub use columns::*;
pub use matching::*;
pub use metrics::*;
pub use rows::*;
