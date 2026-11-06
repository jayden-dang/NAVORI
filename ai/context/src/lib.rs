//! The Context Gateway: key read, Shield pass, audit row, then send.

mod gateway;
mod store_audit;

pub use gateway::*;
pub use store_audit::StoreAudit;
