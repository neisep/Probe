use crate::state::{RequestDraft, ResponseSummary, Result, UIState};

#[derive(Debug)]
pub struct AppState {
    pub ui: UIState,
    pub requests: Vec<RequestDraft>,
    pub responses: Vec<ResponseSummary>,
}

impl AppState {
    pub fn request_id_for_index(index: usize) -> String {
        format!("request-{index}")
    }

    pub fn new() -> Self {
        Self {
            ui: UIState::default(),
            requests: Vec::new(),
            responses: Vec::new(),
        }
    }

    /// Add a request draft to state. Returns its index.
    pub fn add_request(&mut self, r: RequestDraft) -> usize {
        self.requests.push(r);
        self.requests.len() - 1
    }

    /// Try to create and add a request from raw parts.
    pub fn try_add_request(&mut self, method: &str, url: &str) -> Result<usize> {
        let req = RequestDraft::new(method, url)?;
        Ok(self.add_request(req))
    }

    pub fn bootstrap() -> Result<Self> {
        let mut state = Self::new();
        let request_index = state.try_add_request("GET", "https://example.com/health")?;

        state.ui.select_request(request_index);
        state.responses.push(ResponseSummary::pending());

        Ok(state)
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
        if self.requests.is_empty() {
            self.requests.push(RequestDraft::default_request());
        }

        if let Some(selected_response) = self.ui.selected_response {
            if selected_response >= self.responses.len() {
                self.ui.clear_selected_response();
            }
        }

        let selected_request = match self.ui.selected_request {
            Some(selected_request) if selected_request < self.requests.len() => selected_request,
            Some(_) => self.requests.len().saturating_sub(1),
            None => 0,
        };

        self.ui.select_request(selected_request);
    }

    pub fn latest_response(&self) -> Option<&ResponseSummary> {
        self.responses.last()
    }

    pub fn selected_response(&self) -> Option<&ResponseSummary> {
        self.ui
            .selected_response
            .and_then(|selected_response| self.responses.get(selected_response))
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
    use crate::state::RequestDraft;

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
        state.requests[original_index].url = "https://example.com/clone-me".to_owned();

        let duplicated_index = state.duplicate_selected_request();

        assert_eq!(duplicated_index, Some(1));
        assert_eq!(state.ui.selected_request, Some(1));
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
}
