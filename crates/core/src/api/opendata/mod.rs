//! Opendata API (`/api/v1/industry-opendata`).
//!
//! Opendata are public datasets MemoryLake curates by industry — academic
//! papers, SEC filings, clinical trials and so on. A project opts into them by
//! listing their ids on create or update; the project then reports them back
//! expanded. The server still calls the concept "industry" on the wire
//! (`industry_ids`, `industries`); this client names it opendata throughout.

mod list;
mod types;

pub use list::list_opendata;
pub use types::Opendata;
