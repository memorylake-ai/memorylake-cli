//! List industry opendata (`GET /api/v1/industry-opendata`).

use crate::client::Client;
use crate::error::Result;

use super::types::IndustryOpendata;

/// Industry opendata collection endpoint.
const INDUSTRY_OPENDATA_PATH: &str = "/api/v1/industry-opendata";

/// List every industry opendata collection.
///
/// Not paginated: the answer is the whole, short catalogue as a bare array.
pub fn list_industries(client: &Client) -> Result<Vec<IndustryOpendata>> {
    client.get_data(INDUSTRY_OPENDATA_PATH, &[])
}
