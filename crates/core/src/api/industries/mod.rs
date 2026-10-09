//! Industry opendata API (`/api/v1/industry-opendata`).
//!
//! Industry opendata are public datasets MemoryLake curates by industry —
//! academic papers, SEC filings, clinical trials and so on. A project opts into
//! them by listing their ids as `industry_ids` on create or update; the project
//! then reports them back expanded under `industries`.

mod list;
mod types;

pub use list::list_industries;
pub use types::IndustryOpendata;
