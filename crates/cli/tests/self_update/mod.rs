//! `memorylake self update` tests. No API is involved: the wire tests serve
//! releases from a loopback server laid out like GitHub's releases URL.

mod offline;
#[cfg(unix)]
mod wire;
