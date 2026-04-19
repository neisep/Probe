use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum View {
    Editor,
    History,
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
    #[serde(skip)]
    pub request_search_query: String,
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
}
