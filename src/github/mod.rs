pub mod client;
pub mod resolver;

// parse_repo_url is re-exported as the canonical public API for URL parsing.
// Other types are used via their full module paths.
pub use client::parse_repo_url;
