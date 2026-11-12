//! Pure row geometry and search data shared by the list and its header.

mod columns;
mod facets;
#[path = "match.rs"]
mod matching;
mod metrics;
mod row;
mod rows;

pub use columns::*;
pub use facets::*;
pub use matching::*;
pub use metrics::*;
pub use row::*;
pub use rows::*;
