//! Get a single boundary (`GET /api/v3/boundaries/{id}`).

use crate::client::Client;
use crate::error::Result;

use super::boundary_path;
use super::types::Boundary;

/// Fetch a boundary by its server-assigned id.
pub fn get_boundary(client: &Client, id: &str) -> Result<Boundary> {
    client.get_data(&boundary_path(id), &[])
}

/// Fetch a boundary by its caller-defined `custom_id`.
pub fn get_boundary_by_custom_id(client: &Client, custom_id: &str) -> Result<Boundary> {
    client.get_data(
        &boundary_path(custom_id),
        &[("by_custom_id", "true".to_string())],
    )
}
