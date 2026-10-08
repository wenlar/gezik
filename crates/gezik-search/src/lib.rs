//! Searching folder trees (spec 3): the name and content matchers, the results, the scanner,
//! the name cache and Everything. Nothing here touches the UI; the app hands it a
//! `SearchSpec` and takes back batches of results.

pub mod cache;
pub mod content;
pub mod name;
pub mod query;
pub mod results;
pub mod run;
pub mod walk;
