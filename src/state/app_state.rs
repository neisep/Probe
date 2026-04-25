use std::collections::{BTreeMap, BTreeSet};

use crate::state::{
    Environment, RequestDraft, ResponseSummary, Result, StateError, UIState,
    request::normalize_folder_path,
};

#[derive(Debug)]
pub struct AppState {
    pub ui: UIState,
    pub requests: Vec<RequestDraft>,
    pub responses: Vec<ResponseSummary>,
    pub environments: Vec<Environment>,
    pub active_environment: Option<usize>,
}

impl AppState {
    pub fn request_id_for_index(index: usize) -> String {
        format!("request-{index}")
    }

    pub fn new() -> Self {
        let mut state = Self {
            ui: UIState::default(),
            requests: Vec::new(),
            responses: Vec::new(),
            environments: Vec::new(),
            active_environment: None,
        };
        state.ensure_valid_environment_selection();
        state
    }

    pub fn add_request(&mut self, request: RequestDraft) -> usize {
        self.requests.push(request);
        self.requests.len() - 1
    }

    pub fn try_add_request(&mut self, method: &str, url: &str) -> Result<usize> {
        let request = RequestDraft::new(method, url)?;
        Ok(self.add_request(request))
    }

    pub fn bootstrap() -> Result<Self> {
        let mut state = Self::new();
        let request_index = state.try_add_request("GET", "https://example.com/health")?;

        state.ui.select_request(request_index);
        state.responses.push(ResponseSummary::pending());

        Ok(state)
    }

    pub fn find_environment_index(&self, name: &str) -> Option<usize> {
        let normalized_name = name.trim();
        if normalized_name.is_empty() {
            return None;
        }

        self.environments
            .iter()
            .enumerate()
            .find_map(|(index, environment)| (environment.name == normalized_name).then_some(index))
    }

    pub fn add_environment(&mut self, name: &str) -> Result<usize> {
        let environment = Environment::new(name)?;
        if self.find_environment_index(&environment.name).is_some() {
            return Err(StateError::InvalidInput(format!(
                "environment '{}' already exists",
                environment.name
            )));
        }

        self.environments.push(environment);
        let index = self.environments.len() - 1;
        if self.active_environment_index().is_none() {
            self.active_environment = Some(index);
        }

        Ok(index)
    }

    pub fn select_environment(&mut self, name: &str) -> Option<usize> {
        let environment_index = self.find_environment_index(name)?;
        self.active_environment = Some(environment_index);
        Some(environment_index)
    }

    pub fn remove_environment(&mut self, name: &str) -> bool {
        let Some(environment_index) = self.find_environment_index(name) else {
            self.ensure_valid_environment_selection();
            return false;
        };

        let active_environment = self.active_environment_index();
        self.environments.remove(environment_index);

        if self.environments.is_empty() {
            self.environments.push(Environment::default());
            self.active_environment = Some(0);
            return true;
        }

        let next_active_environment = match active_environment {
            Some(active_environment) if active_environment == environment_index => {
                environment_index.min(self.environments.len() - 1)
            }
            Some(active_environment) if active_environment > environment_index => {
                active_environment - 1
            }
            Some(active_environment) => active_environment,
            None => 0,
        };

        self.active_environment = Some(next_active_environment);
        true
    }

    pub fn active_environment_index(&self) -> Option<usize> {
        self.active_environment
            .filter(|&active_environment| active_environment < self.environments.len())
    }

    pub fn active_environment(&self) -> Option<&Environment> {
        self.active_environment_index()
            .and_then(|active_environment| self.environments.get(active_environment))
    }

    pub fn active_environment_mut(&mut self) -> Option<&mut Environment> {
        self.active_environment_index()
            .and_then(|active_environment| self.environments.get_mut(active_environment))
    }

    pub fn active_environment_name(&self) -> Option<&str> {
        self.active_environment()
            .map(|environment| environment.name.as_str())
    }

    pub fn active_variables(&self) -> Option<&BTreeMap<String, String>> {
        self.active_environment()
            .map(|environment| &environment.vars)
    }

    #[allow(dead_code)]
    pub fn active_variables_mut(&mut self) -> Option<&mut BTreeMap<String, String>> {
        self.active_environment_mut()
            .map(|environment| &mut environment.vars)
    }

    pub fn set_active_environment_var(&mut self, key: &str, value: &str) -> Result<Option<String>> {
        self.ensure_valid_environment_selection();
        match self.active_environment_mut() {
            Some(environment) => environment.set_var(key, value),
            None => Err(StateError::InvalidInput(
                "active environment is unavailable".to_owned(),
            )),
        }
    }

    #[allow(dead_code)]
    pub fn remove_active_environment_var(&mut self, key: &str) -> Option<String> {
        self.active_environment_mut()
            .and_then(|environment| environment.remove_var(key))
    }

    pub fn selected_request_index(&self) -> Option<usize> {
        self.ui
            .selected_request
            .filter(|&selected_request| selected_request < self.requests.len())
    }

    pub fn selected_request(&self) -> Option<&RequestDraft> {
        self.selected_request_index()
            .and_then(|selected_request| self.requests.get(selected_request))
    }

    #[allow(dead_code)]
    pub fn selected_request_id(&self) -> Option<String> {
        self.selected_request_index()
            .map(Self::request_id_for_index)
    }

    pub fn selected_request_mut(&mut self) -> Option<&mut RequestDraft> {
        self.selected_request_index()
            .and_then(|selected_request| self.requests.get_mut(selected_request))
    }

    pub fn find_request_index_by_id(&self, request_id: &str) -> Option<usize> {
        self.requests
            .iter()
            .enumerate()
            .find_map(|(index, _request)| {
                (Self::request_id_for_index(index) == request_id).then_some(index)
            })
    }

    pub fn request_name(&self, index: usize) -> Option<&str> {
        self.requests.get(index).and_then(|request| {
            let name = request.name.trim();
            (!name.is_empty()).then_some(name)
        })
    }

    pub fn request_folder_path(&self, index: usize) -> Option<&str> {
        self.requests.get(index).and_then(RequestDraft::folder_path)
    }

    #[allow(dead_code)]
    pub fn set_request_organization(
        &mut self,
        index: usize,
        name: &str,
        folder_path: &str,
    ) -> bool {
        let Some(request) = self.requests.get_mut(index) else {
            return false;
        };

        request.set_organization(name, folder_path);
        true
    }

    #[allow(dead_code)]
    pub fn set_selected_request_organization(&mut self, name: &str, folder_path: &str) -> bool {
        let Some(request) = self.selected_request_mut() else {
            return false;
        };

        request.set_organization(name, folder_path);
        true
    }

    pub fn request_indices_by_folder(&self) -> BTreeMap<String, Vec<usize>> {
        let mut grouped_requests: BTreeMap<String, Vec<usize>> = BTreeMap::new();

        for (index, request) in self.requests.iter().enumerate() {
            grouped_requests
                .entry(normalize_folder_path(&request.folder))
                .or_insert_with(Vec::new)
                .push(index);
        }

        grouped_requests
    }

    pub fn folder_paths(&self) -> Vec<String> {
        let mut folders = BTreeSet::new();

        for request in &self.requests {
            let folder = normalize_folder_path(&request.folder);
            if !folder.is_empty() {
                folders.insert(folder);
            }
        }

        folders.into_iter().collect()
    }

    pub fn add_default_request(&mut self) -> usize {
        let index = self.add_request(RequestDraft::default_request());
        self.ui.select_request(index);
        self.ui.clear_selected_response();
        self.ensure_valid_selection();
        index
    }

    pub fn duplicate_selected_request(&mut self) -> Option<usize> {
        let selected_request = self.selected_request_index()?;
        let duplicated_request = self.requests.get(selected_request)?.duplicate();
        let duplicated_index = self.add_request(duplicated_request);
        self.ui.select_request(duplicated_index);
        self.ui.clear_selected_response();
        self.ensure_valid_selection();
        Some(duplicated_index)
    }

    pub fn remove_selected_request(&mut self) -> bool {
        let Some(selected_request) = self.selected_request_index() else {
            self.ensure_valid_selection();
            return false;
        };

        self.requests.remove(selected_request);

        if self.requests.is_empty() {
            self.requests.push(RequestDraft::default_request());
            self.ui.select_request(0);
        } else {
            let next_selection = selected_request.min(self.requests.len() - 1);
            self.ui.select_request(next_selection);
        }

        self.ui.clear_selected_response();
        self.ensure_valid_selection();
        true
    }

    pub fn ensure_valid_selection(&mut self) {
        self.ensure_valid_environment_selection();

        if self.requests.is_empty() {
            self.requests.push(RequestDraft::default_request());
        }

        if let Some(selected_response) = self.ui.selected_response
            && selected_response >= self.responses.len()
        {
            self.ui.clear_selected_response();
        }

        let selected_request = match self.ui.selected_request {
            Some(selected_request) if selected_request < self.requests.len() => selected_request,
            Some(_) => self.requests.len().saturating_sub(1),
            None => 0,
        };

        self.ui.select_request(selected_request);
    }

    pub fn ensure_valid_environment_selection(&mut self) {
        if self.environments.is_empty() {
            self.environments.push(Environment::default());
        }

        let active_environment = match self.active_environment {
            Some(active_environment) if active_environment < self.environments.len() => {
                active_environment
            }
            Some(_) => self.environments.len().saturating_sub(1),
            None => 0,
        };

        self.active_environment = Some(active_environment);
    }

    pub fn latest_response(&self) -> Option<&ResponseSummary> {
        self.responses.last()
    }

    pub fn selected_response(&self) -> Option<&ResponseSummary> {
        self.ui
            .selected_response
            .and_then(|selected_response| self.responses.get(selected_response))
    }

    pub fn responses_for_selected_request(&self) -> Vec<usize> {
        let Some(request_index) = self.ui.selected_request else {
            return Vec::new();
        };
        let request_id = Self::request_id_for_index(request_index);
        self.responses
            .iter()
            .enumerate()
            .filter_map(|(index, response)| {
                (response.request_id.as_deref() == Some(request_id.as_str())).then_some(index)
            })
            .collect()
    }

    pub fn latest_response_for_selected_request(&self) -> Option<&ResponseSummary> {
        self.responses_for_selected_request()
            .last()
            .and_then(|&index| self.responses.get(index))
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::AppState;
    use crate::state::{Environment, RequestDraft, StateError};

    #[test]
    fn add_default_request_selects_new_request() {
        let mut state = AppState::new();

        let index = state.add_default_request();

        assert_eq!(index, 0);
        assert_eq!(state.ui.selected_request, Some(0));
        assert_eq!(
            state.selected_request().map(RequestDraft::display_name),
            Some("GET https://example.com".to_owned())
        );
    }

    #[test]
    fn duplicate_selected_request_selects_clone() {
        let mut state = AppState::new();
        let original_index = state.add_default_request();
        state.requests[original_index].name = "Clone me".to_owned();
        state.requests[original_index].folder = "Examples".to_owned();
        state.requests[original_index].url = "https://example.com/clone-me".to_owned();

        let duplicated_index = state.duplicate_selected_request();

        assert_eq!(duplicated_index, Some(1));
        assert_eq!(state.ui.selected_request, Some(1));
        assert_eq!(state.requests[1].name, "Clone me");
        assert_eq!(state.requests[1].folder, "Examples");
        assert_eq!(state.requests[1].url, "https://example.com/clone-me");
    }

    #[test]
    fn remove_selected_request_keeps_one_default_request() {
        let mut state = AppState::new();
        state.add_default_request();

        let removed = state.remove_selected_request();

        assert!(removed);
        assert_eq!(state.requests.len(), 1);
        assert_eq!(state.ui.selected_request, Some(0));
        assert_eq!(state.requests[0].display_name(), "GET https://example.com");
    }

    #[test]
    fn ensure_valid_selection_clamps_invalid_request_index() {
        let mut state = AppState::new();
        state.add_default_request();
        state.add_default_request();
        state.ui.selected_request = Some(9);

        state.ensure_valid_selection();

        assert_eq!(state.ui.selected_request, Some(1));
    }

    #[test]
    fn add_request_preserves_request_metadata() {
        let mut state = AppState::new();
        let mut draft = RequestDraft::default_request();
        draft.name = "Health check".to_owned();
        draft.folder = "System".to_owned();

        let index = state.add_request(draft);

        assert_eq!(state.request_name(index), Some("Health check"));
        assert_eq!(state.request_folder_path(index), Some("System"));
        assert_eq!(state.requests[index].display_name(), "Health check");
    }

    #[test]
    fn request_indices_by_folder_groups_ungrouped_requests() {
        let mut state = AppState::new();
        let first = state.add_default_request();
        let second = state.add_default_request();
        let third = state.add_default_request();

        state.requests[first].name = "Health".to_owned();
        state.requests[first].folder = "System".to_owned();
        state.requests[second].name = "Users".to_owned();
        state.requests[second].folder = "System".to_owned();
        state.requests[third].name = "Root".to_owned();
        state.requests[third].folder = "  ".to_owned();

        let grouped = state.request_indices_by_folder();

        assert_eq!(grouped.get("System"), Some(&vec![0, 1]));
        assert_eq!(grouped.get(""), Some(&vec![2]));
    }

    #[test]
    fn folder_paths_are_sorted_and_normalized() {
        let mut state = AppState::new();
        let first = state.add_default_request();
        let second = state.add_default_request();
        let third = state.add_default_request();

        state.requests[first].folder = "  Collections / API ".to_owned();
        state.requests[second].folder = "Collections//API/Health".to_owned();
        state.requests[third].folder = "Collections\\Auth".to_owned();

        assert_eq!(
            state.folder_paths(),
            vec![
                "Collections/API".to_owned(),
                "Collections/API/Health".to_owned(),
                "Collections/Auth".to_owned(),
            ]
        );
    }

    #[test]
    fn new_state_starts_with_default_environment_selected() {
        let state = AppState::new();

        assert_eq!(state.environments, vec![Environment::default()]);
        assert_eq!(state.active_environment, Some(0));
        assert_eq!(state.active_environment_name(), Some("Default"));
        assert_eq!(state.active_variables().map(|vars| vars.len()), Some(0));
    }

    #[test]
    fn add_select_and_remove_environments_are_safe() {
        let mut state = AppState::new();

        let added_index = state.add_environment("Local");
        assert!(matches!(added_index, Ok(1)));
        assert_eq!(state.select_environment("Local"), Some(1));
        assert_eq!(state.active_environment_name(), Some("Local"));

        let duplicate = state.add_environment("Local");
        assert!(matches!(
            duplicate,
            Err(StateError::InvalidInput(message)) if message.contains("already exists")
        ));

        assert!(state.remove_environment("Local"));
        assert_eq!(state.active_environment_name(), Some("Default"));
        assert_eq!(state.environments.len(), 1);
    }

    #[test]
    fn removing_last_environment_restores_default_environment() {
        let mut state = AppState::new();

        let _old_value = state.set_active_environment_var("base_url", "https://example.com");
        assert!(state.remove_environment("Default"));

        assert_eq!(state.environments, vec![Environment::default()]);
        assert_eq!(state.active_environment_name(), Some("Default"));
        assert_eq!(state.active_variables().map(|vars| vars.len()), Some(0));
    }

    #[test]
    fn active_environment_variables_follow_active_selection() {
        let mut state = AppState::new();

        let initial_value = state.set_active_environment_var("token", "abc123");
        assert!(matches!(initial_value, Ok(None)));
        assert_eq!(
            state
                .active_environment()
                .and_then(|environment| environment.get_var("token")),
            Some("abc123")
        );

        assert!(matches!(state.add_environment("Staging"), Ok(1)));
        assert_eq!(state.select_environment("Staging"), Some(1));
        assert!(matches!(
            state.set_active_environment_var("token", "staging"),
            Ok(None)
        ));
        assert_eq!(
            state
                .active_environment()
                .and_then(|environment| environment.get_var("token")),
            Some("staging")
        );

        assert_eq!(state.select_environment("Default"), Some(0));
        assert_eq!(
            state
                .active_environment()
                .and_then(|environment| environment.get_var("token")),
            Some("abc123")
        );
    }
}
