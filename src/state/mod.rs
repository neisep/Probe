pub mod app_state;
pub mod environment;
pub mod request;
pub mod response;
pub mod ui_state;

pub use app_state::AppState;
pub use environment::Environment;
pub use request::RequestDraft;
pub use response::ResponseSummary;
pub use ui_state::{RequestTab, UIState, View};

use std::fmt;

#[derive(Debug)]
pub enum StateError {
    InvalidInput(String),
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateError::InvalidInput(s) => write!(f, "invalid input: {}", s),
        }
    }
}

impl std::error::Error for StateError {}

pub type Result<T> = std::result::Result<T, StateError>;
