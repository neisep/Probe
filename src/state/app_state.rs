use crate::state::{RequestDraft, ResponseSummary, Result, UIState};

#[derive(Debug)]
pub struct AppState {
    pub ui: UIState,
    pub requests: Vec<RequestDraft>,
    pub responses: Vec<ResponseSummary>,
}

impl AppState {
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

    pub fn selected_request(&self) -> Option<&RequestDraft> {
        self.ui
            .selected_request
            .and_then(|selected_request| self.requests.get(selected_request))
    }

    pub fn selected_request_mut(&mut self) -> Option<&mut RequestDraft> {
        self.ui
            .selected_request
            .and_then(|selected_request| self.requests.get_mut(selected_request))
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
