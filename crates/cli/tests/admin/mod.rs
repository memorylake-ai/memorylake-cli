//! Test suite for the team-management commands
//! (`team`, `api-key`, `member`, `invitation`, `usage`) and the
//! access-control ones (`role`, `access-group`, `permission`, `access`),
//! plus `billing` and `audit-log`.

mod live;
mod live_access;
mod live_billing;
mod offline;
mod wire;
mod wire_access;
mod wire_billing;
