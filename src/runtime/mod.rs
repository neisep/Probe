// Runtime module boundary reserved for async send flow work.
//
// Public API:
// - Runtime: create with Runtime::new(buffer_size)
// - submit(request) -> RequestId
// - poll_events() -> Vec<Event> (drains internal event queue)
// - get_status(id) -> Option<RequestStatus>
// - cancel(id) -> bool (best-effort placeholder)

pub mod executor;
pub mod types;

pub use crate::runtime::executor::Runtime;
pub use crate::runtime::types::*;
