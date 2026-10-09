//! Analysis model v3 API (`/api/v3/workspaces/{workspace_id}/analysis-models`).
//!
//! An analysis model is curated knowledge about one database datasource: the
//! business rules, worked question-to-SQL examples, metric definitions and
//! background notes that help questions about that data be answered well. Each
//! model follows a *template* (its `type`, e.g. `ASK_DATA`), and the template
//! decides which kinds of knowledge — `entity_type`s — the model accepts.
//!
//! Knowledge lives in *entries*. Every entry has an `entity_type`, an
//! `embedding` (the text an incoming question is matched against) and a
//! `payload` whose shape follows the kind. Entries are listed one kind at a
//! time, and some kinds cannot be located from an entry id alone, so most entry
//! calls take the kind as well.

mod create;
mod create_entry;
mod delete;
mod delete_entry;
mod get;
mod get_entry;
mod list;
mod list_entries;
mod path;
mod set_entry_disabled;
mod templates;
mod types;
mod update;
mod update_entry;

pub use create::{CreateAnalysisModelRequest, create_analysis_model};
pub use create_entry::{CreateEntryRequest, create_entry};
pub use delete::delete_analysis_model;
pub use delete_entry::delete_entry;
pub use get::{get_analysis_model, get_analysis_model_by_custom_id};
pub use get_entry::get_entry;
pub use list::{AnalysisModelList, ListAnalysisModelsParams, list_analysis_models};
pub use list_entries::{KnowledgeEntryList, ListEntriesParams, list_entries};
pub use set_entry_disabled::{SetEntryDisabledRequest, set_entry_disabled};
pub use templates::list_analysis_model_templates;
pub use types::{AnalysisModel, AnalysisModelTemplate, KnowledgeEntry, KnowledgeEntryType};
pub use update::{UpdateAnalysisModelRequest, update_analysis_model};
pub use update_entry::{UpdateEntryRequest, update_entry};
