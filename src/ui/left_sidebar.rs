#[path = "environment_editor.rs"]
pub mod environment_editor;

use crate::state::{AppState, RequestDraft, View, request::normalize_folder_path};
use crate::ui::theme;
use eframe::egui;
use std::collections::BTreeMap;

fn compact_text(text: &str, max: usize) -> String {
    let mut compact = text.trim().to_owned();
    if compact.len() > max {
        compact.truncate(max.saturating_sub(3));
        compact.push_str("...");
    }
    compact
}

fn request_label(req: &crate::state::RequestDraft) -> String {
    let name = req.name.trim();
    if name.is_empty() {
        req.display_name()
    } else {
        name.to_owned()
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct FolderTreeNode {
    requests: Vec<usize>,
    children: BTreeMap<String, FolderTreeNode>,
}

impl FolderTreeNode {
    fn insert_request(&mut self, folder_path: &str, request_index: usize) {
        if folder_path.is_empty() {
            self.requests.push(request_index);
            return;
        }

        let mut node = self;
        for segment in folder_path.split('/') {
            node = node.children.entry(segment.to_owned()).or_default();
        }
        node.requests.push(request_index);
    }

    fn total_request_count(&self) -> usize {
        self.requests.len()
            + self
                .children
                .values()
                .map(FolderTreeNode::total_request_count)
                .sum::<usize>()
    }

    fn contains_request(&self, selected_index: usize) -> bool {
        self.requests.contains(&selected_index)
            || self
                .children
                .values()
                .any(|child| child.contains_request(selected_index))
    }
}

fn normalized_search_query(query: &str) -> Option<String> {
    let query = query.trim().to_ascii_lowercase();
    (!query.is_empty()).then_some(query)
}

fn request_matches_query(request: &RequestDraft, query: &str) -> bool {
    let folder = normalize_folder_path(&request.folder);
    [
        request.name.as_str(),
        request.method.as_str(),
        request.url.as_str(),
        folder.as_str(),
    ]
    .into_iter()
    .any(|field| field.to_ascii_lowercase().contains(query))
}

fn build_folder_tree(state: &AppState, search_query: Option<&str>) -> FolderTreeNode {
    let mut root = FolderTreeNode::default();

    for (index, request) in state.requests.iter().enumerate() {
        if let Some(search_query) = search_query
            && !request_matches_query(request, search_query)
        {
            continue;
        }
        root.insert_request(&normalize_folder_path(&request.folder), index);
    }

    root
}

fn show_request_row(
    ui: &mut egui::Ui,
    state: &mut AppState,
    index: usize,
    selected_index: Option<usize>,
) {
    let Some(req) = state.requests.get(index) else {
        return;
    };

    let is_selected = selected_index == Some(index);
    let display_name = compact_text(&request_label(req), 40);
    let method = req.method.clone();
    let mut select_request = false;

    ui.horizontal(|ui| {
        ui.label(theme::method_badge(&method));
        if ui.selectable_label(is_selected, display_name).clicked() {
            select_request = true;
        }
    });

    if select_request {
        state.ui.select_request(index);
        state.ui.set_view(View::Editor);
    }
}

fn show_folder_node(
    ui: &mut egui::Ui,
    state: &mut AppState,
    selected_index: Option<usize>,
    search_active: bool,
    path_prefix: &str,
    segment_name: &str,
    node: &FolderTreeNode,
) {
    let full_path = if path_prefix.is_empty() {
        segment_name.to_owned()
    } else {
        format!("{path_prefix}/{segment_name}")
    };
    let contains_selected =
        selected_index.is_some_and(|selected_index| node.contains_request(selected_index));
    let response = egui::CollapsingHeader::new(format!(
        "{} ({})",
        compact_text(segment_name, 28),
        node.total_request_count()
    ))
    .id_salt(format!("folder::{full_path}"))
    .default_open(search_active || contains_selected)
    .show(ui, |ui| {
        for index in &node.requests {
            show_request_row(ui, state, *index, selected_index);
        }

        for (child_name, child_node) in &node.children {
            show_folder_node(
                ui,
                state,
                selected_index,
                search_active,
                &full_path,
                child_name,
                child_node,
            );
        }
    });
    response.header_response.on_hover_text(full_path);
}

pub fn show_sidebar(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::left("sidebar")
        .resizable(true)
        .default_size(260.0)
        .show_inside(ui, |ui| {
            let has_selected_request = state.selected_request_index().is_some();

            ui.horizontal(|ui| {
                ui.heading("Requests");
                ui.add_space(8.0);

                if ui
                    .small_button("New")
                    .on_hover_text("Create a fresh request draft")
                    .clicked()
                {
                    let new_index = state.add_default_request();
                    state.ui.select_request(new_index);
                    state.ui.set_view(View::Editor);
                }

                if ui
                    .add_enabled(has_selected_request, egui::Button::new("Dup").small())
                    .on_hover_text("Duplicate the selected request draft")
                    .clicked()
                {
                    if let Some(new_index) = state.duplicate_selected_request() {
                        state.ui.select_request(new_index);
                        state.ui.set_view(View::Editor);
                    }
                }

                if ui
                    .add_enabled(has_selected_request, egui::Button::new("Del").small())
                    .on_hover_text("Delete the selected request draft")
                    .clicked()
                {
                    let _removed = state.remove_selected_request();
                    state.ui.set_view(View::Editor);
                }
            });
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Search");
                ui.add(
                    egui::TextEdit::singleline(&mut state.ui.request_search_query)
                        .hint_text("Name, folder, method, or URL")
                        .desired_width(f32::INFINITY),
                );
                if state.ui.has_request_search() && ui.small_button("Clear").clicked() {
                    state.ui.clear_request_search();
                }
            });
            ui.add_space(4.0);

            let selected_index = state.selected_request_index();
            let search_query = normalized_search_query(&state.ui.request_search_query);
            let search_active = search_query.is_some();

            if state.requests.is_empty() {
                ui.label("No requests yet");
            } else {
                let folder_tree = build_folder_tree(state, search_query.as_deref());
                let has_ungrouped = !folder_tree.requests.is_empty();
                let matching_request_count = folder_tree.total_request_count();

                if search_active {
                    if matching_request_count == 0 {
                        ui.small(format!(
                            "No requests match '{}'.",
                            state.ui.request_search_query.trim()
                        ));
                    } else {
                        ui.small(format!(
                            "Showing {matching_request_count} matching request{}.",
                            if matching_request_count == 1 { "" } else { "s" }
                        ));
                    }
                    ui.add_space(4.0);
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    if matching_request_count == 0 {
                        return;
                    }

                    if has_ungrouped {
                        if !folder_tree.children.is_empty() {
                            ui.small("Ungrouped");
                            ui.add_space(4.0);
                        }

                        for index in &folder_tree.requests {
                            show_request_row(ui, state, *index, selected_index);
                        }
                    }

                    if !folder_tree.children.is_empty() {
                        if has_ungrouped {
                            ui.add_space(6.0);
                            ui.separator();
                            ui.add_space(2.0);
                        }

                        for (folder_name, folder_node) in &folder_tree.children {
                            show_folder_node(
                                ui,
                                state,
                                selected_index,
                                search_active,
                                "",
                                folder_name,
                                folder_node,
                            );
                        }
                    }
                });
            }

        });
}

#[cfg(test)]
mod tests {
    use super::{build_folder_tree, request_matches_query};
    use crate::state::{AppState, RequestDraft};

    #[test]
    fn folder_tree_keeps_root_requests_separate_from_nested_paths() {
        let mut state = AppState::new();
        let root = state.add_default_request();
        let nested = state.add_default_request();
        let sibling = state.add_default_request();

        state.requests[root].folder = String::new();
        state.requests[nested].folder = "Collections/API".to_owned();
        state.requests[sibling].folder = "Collections/Auth".to_owned();

        let tree = build_folder_tree(&state, None);

        assert_eq!(tree.requests, vec![root]);
        let collections = tree.children.get("Collections").expect("collections node");
        assert!(collections.requests.is_empty());
        assert_eq!(
            collections.children.get("API").expect("api node").requests,
            vec![nested]
        );
        assert_eq!(
            collections
                .children
                .get("Auth")
                .expect("auth node")
                .requests,
            vec![sibling]
        );
    }

    #[test]
    fn request_search_matches_name_folder_method_and_url() {
        let mut request = RequestDraft::default_request();
        request.set_request_name("Create user");
        request.set_folder_path("Collections/Auth");
        request.method = "POST".to_owned();
        request.set_url("https://example.com/users");

        assert!(request_matches_query(&request, "create"));
        assert!(request_matches_query(&request, "auth"));
        assert!(request_matches_query(&request, "post"));
        assert!(request_matches_query(&request, "users"));
        assert!(!request_matches_query(&request, "billing"));
    }

    #[test]
    fn folder_tree_filter_omits_non_matching_branches() {
        let mut state = AppState::new();
        let auth_request = state.add_default_request();
        let api_request = state.add_default_request();

        state.requests[auth_request].set_request_name("Login");
        state.requests[auth_request].set_folder_path("Collections/Auth");
        state.requests[api_request].set_request_name("List widgets");
        state.requests[api_request].set_folder_path("Collections/API");

        let tree = build_folder_tree(&state, Some("auth"));

        let collections = tree.children.get("Collections").expect("collections node");
        assert_eq!(
            collections
                .children
                .get("Auth")
                .expect("auth node")
                .requests,
            vec![auth_request]
        );
        assert!(!collections.children.contains_key("API"));
    }
}
