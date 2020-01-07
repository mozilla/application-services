#[macro_use]
mod util;
pub mod engine;
pub mod error;
pub mod ms_time;
pub mod schema;
pub mod storage;
pub mod vclock;

// Some re-exports we use frequently for local convenience
pub(crate) use sync_guid::Guid;

pub(crate) use serde_json::Value as JsonValue;
pub(crate) type JsonObject<Val = JsonValue> = serde_json::Map<String, Val>;

pub use crate::engine::RemergeEngine;
pub use crate::error::*;
pub use crate::ms_time::MsTime;
pub use crate::vclock::VClock;
