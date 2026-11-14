//! HTTP fetch layer. Lives in the UI crate because its client is gpui_kit's
//! `http_client` re-export; the URL/version rules stay in `manyproxy_core`.

pub mod fetch;
pub mod passive;

pub use fetch::*;
pub use passive::*;
