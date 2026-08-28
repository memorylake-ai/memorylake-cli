//! Actor v3 API (`/api/v3/actors`, plus workspace bindings).

mod bindings;
mod create;
mod delete;
mod get;
mod list;
mod me;
mod types;
mod update;

pub use bindings::{WorkspaceActorList, bind_actor, list_workspace_actors, unbind_actor};
pub use create::{CreateActorRequest, create_actor};
pub use delete::delete_actor;
pub use get::{get_actor, get_actor_by_custom_id};
pub use list::{ActorList, ListActorsParams, list_actors};
pub use me::get_my_actor;
pub use types::{ACTOR_TYPE_ASSISTANT, ACTOR_TYPE_HUMAN, Actor, ActorBinding, ActorType};
pub use update::{UpdateActorRequest, update_actor};
