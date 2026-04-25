use std::collections::BTreeMap;

use crate::state::{Result, StateError};
use serde::{Deserialize, Serialize};

pub const DEFAULT_ENVIRONMENT_NAME: &str = "Default";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    pub name: String,
    pub vars: BTreeMap<String, String>,
}

impl Environment {
    pub fn new(name: &str) -> Result<Self> {
        let normalized_name = name.trim();
        if normalized_name.is_empty() {
            return Err(StateError::InvalidInput(
                "environment name cannot be empty".to_owned(),
            ));
        }

        Ok(Self {
            name: normalized_name.to_owned(),
            vars: BTreeMap::new(),
        })
    }

    pub fn set_var(&mut self, key: &str, value: &str) -> Result<Option<String>> {
        let normalized_key = key.trim();
        if normalized_key.is_empty() {
            return Err(StateError::InvalidInput(
                "environment variable key cannot be empty".to_owned(),
            ));
        }

        Ok(self
            .vars
            .insert(normalized_key.to_owned(), value.to_owned()))
    }

    #[allow(dead_code)]
    pub fn remove_var(&mut self, key: &str) -> Option<String> {
        let normalized_key = key.trim();
        if normalized_key.is_empty() {
            return None;
        }

        self.vars.remove(normalized_key)
    }

    pub fn get_var(&self, key: &str) -> Option<&str> {
        let normalized_key = key.trim();
        if normalized_key.is_empty() {
            return None;
        }

        self.vars.get(normalized_key).map(String::as_str)
    }
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            name: DEFAULT_ENVIRONMENT_NAME.to_owned(),
            vars: BTreeMap::new(),
        }
    }
}
