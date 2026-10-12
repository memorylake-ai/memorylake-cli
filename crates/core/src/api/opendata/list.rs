//! List opendata (`GET /api/v1/industry-opendata`).

use crate::client::Client;
use crate::error::Result;

use super::types::Opendata;

/// Opendata collection endpoint.
const OPENDATA_PATH: &str = "/api/v1/industry-opendata";

/// List every opendata collection.
///
/// Not paginated: the answer is the whole, short catalogue as a bare array.
pub fn list_opendata(client: &Client) -> Result<Vec<Opendata>> {
    client.get_data(OPENDATA_PATH, &[])
}
