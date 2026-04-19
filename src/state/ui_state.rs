use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestUiAction {
    PreviewRequest(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum View {
    Editor,
    History,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestTab {
    #[default]
    Params,
    Auth,
    Headers,
    Body,
}

impl RequestTab {
    pub const ALL: [Self; 4] = [Self::Params, Self::Auth, Self::Headers, Self::Body];

    pub fn label(self) -> &'static str {
        match self {
            Self::Params => "Params",
            Self::Auth => "Auth",
            Self::Headers => "Headers",
            Self::Body => "Body",
        }
    }
}

impl View {
    pub fn label(self) -> &'static str {
        match self {
            View::Editor => "Editor",
            View::History => "History",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "Editor" => Some(View::Editor),
            "History" => Some(View::History),
            _ => None,
        }
    }
}

impl Default for View {
    fn default() -> Self {
        View::Editor
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UIState {
    pub selected_request: Option<usize>,
    pub selected_response: Option<usize>,
    pub view: View,
    #[serde(default)]
    pub request_tab: RequestTab,
    #[serde(skip)]
    pub request_search_query: String,
    #[serde(skip)]
    pub settings_open: bool,
    #[serde(skip)]
    pending_request_action: Option<RequestUiAction>,
}

impl UIState {
    pub fn select_request(&mut self, selected_request: usize) {
        self.selected_request = Some(selected_request);
    }

    pub fn select_response(&mut self, selected_response: usize) {
        self.selected_response = Some(selected_response);
    }

    pub fn clear_selected_response(&mut self) {
        self.selected_response = None;
    }

    pub fn set_view(&mut self, view: View) {
        self.view = view;
    }

    pub fn has_request_search(&self) -> bool {
        !self.request_search_query.trim().is_empty()
    }

    pub fn clear_request_search(&mut self) {
        self.request_search_query.clear();
    }

    pub fn queue_preview_request(&mut self, request_index: usize) {
        self.pending_request_action = Some(RequestUiAction::PreviewRequest(request_index));
    }

    pub fn take_pending_request_action(&mut self) -> Option<RequestUiAction> {
        self.pending_request_action.take()
    }
}
