//! HTTP fetch layer. Lives in the UI crate because its client is gpui_kit's
//! `http_client` re-export; the URL/version rules stay in `manyproxy_core`.

mod fetch;
mod passive;

pub use fetch::*;
pub use passive::*;
