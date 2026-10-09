//! Memory settings (`.../{actors|projects|agents}/{id}/settings`).
//!
//! Each actor, project, and agent carries a *fact instruction*: Markdown that
//! says what its memory is about and what it is not, narrowing what the
//! server records there. The draft endpoint asks a language model to write a
//! candidate without saving it.

mod draft;
mod get;
mod types;
mod update;

pub use draft::{DRAFT_TIMEOUT, draft_fact_instruction};
pub use get::get_memory_settings;
pub use types::{
    DraftFactInstructionRequest, FactInstructionDraft, MAX_DRAFT_GUIDANCE_LEN,
    MAX_DRAFT_LANGUAGE_LEN, MAX_FACT_INSTRUCTION_LEN, MemorySettings, UpdateMemorySettingsRequest,
};
pub use update::update_memory_settings;
