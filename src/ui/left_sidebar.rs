#[path = "environment_editor.rs"]
pub mod environment_editor;

use crate::state::{AppState, View, request::normalize_folder_path};
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

fn method_color(method: &str) -> egui::Color32 {
    match method {
        "GET" => egui::Color32::from_rgb(88, 165, 77),
        "POST" => egui::Color32::from_rgb(66, 133, 244),
        "PUT" => egui::Color32::from_rgb(244, 180, 0),
        "DELETE" => egui::Color32::from_rgb(219, 68, 55),
        _ => egui::Color32::LIGHT_GRAY,
    }
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

fn build_folder_tree(state: &AppState) -> FolderTreeNode {
    let mut root = FolderTreeNode::default();

    for (index, request) in state.requests.iter().enumerate() {
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

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(" {} ", method))
                .monospace()
                .strong()
                .color(method_color(&method))
                .background_color(egui::Color32::from_black_alpha(12)),
        );

        if ui.selectable_label(is_selected, display_name).clicked() {
            state.ui.select_request(index);
            state.ui.set_view(View::Editor);
        }
    });
}

fn show_folder_node(
    ui: &mut egui::Ui,
    state: &mut AppState,
    selected_index: Option<usize>,
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
    .default_open(contains_selected)
    .show(ui, |ui| {
        for index in &node.requests {
            show_request_row(ui, state, *index, selected_index);
        }

        for (child_name, child_node) in &node.children {
            show_folder_node(
                ui,
                state,
                selected_index,
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

            let selected_index = state.selected_request_index();

            if state.requests.is_empty() {
                ui.label("No requests yet");
            } else {
                let folder_tree = build_folder_tree(state);
                let has_ungrouped = !folder_tree.requests.is_empty();

                egui::ScrollArea::vertical().show(ui, |ui| {
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
                                "",
                                folder_name,
                                folder_node,
                            );
                        }
                    }
                });
            }

            ui.add_space(8.0);
            ui.separator();
            environment_editor::show_sidebar_section(ui, state);
            ui.add_space(8.0);
            ui.separator();
            ui.heading("Summary");
            ui.label(format!("Requests: {}", state.requests.len()));
            ui.label(format!("Responses: {}", state.responses.len()));
            if let Some(index) = selected_index {
                ui.label(format!("Selected index: {}", index));
            } else {
                ui.label("Selected index: -");
            }
            ui.add_space(8.0);

            ui.heading("Views");
            ui.separator();
            for v in [View::Editor, View::History] {
                let is_selected = state.ui.view == v;
                if ui.selectable_label(is_selected, v.label()).clicked() {
                    state.ui.set_view(v);
                }
            }

            ui.add_space(10.0);
            ui.heading("Shortcuts");
            ui.separator();
            ui.label("• New/Dup/Del: sidebar buttons");
            ui.label("• Send: Use bottom 'Send selected request' button");
        });
}

#[cfg(test)]
mod tests {
    use super::build_folder_tree;
    use crate::state::AppState;

    #[test]
    fn folder_tree_keeps_root_requests_separate_from_nested_paths() {
        let mut state = AppState::new();
        let root = state.add_default_request();
        let nested = state.add_default_request();
        let sibling = state.add_default_request();

        state.requests[root].folder = String::new();
        state.requests[nested].folder = "Collections/API".to_owned();
        state.requests[sibling].folder = "Collections/Auth".to_owned();

        let tree = build_folder_tree(&state);

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
}
