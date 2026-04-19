use eframe::egui;

pub const BG: egui::Color32 = egui::Color32::from_rgb(0x1a, 0x1b, 0x26);
pub const PANEL: egui::Color32 = egui::Color32::from_rgb(0x1f, 0x20, 0x2e);
pub const SURFACE: egui::Color32 = egui::Color32::from_rgb(0x24, 0x26, 0x35);
pub const SURFACE_HOVER: egui::Color32 = egui::Color32::from_rgb(0x2e, 0x30, 0x41);
pub const BORDER: egui::Color32 = egui::Color32::from_rgb(0x33, 0x35, 0x4a);
pub const TEXT: egui::Color32 = egui::Color32::from_rgb(0xc0, 0xca, 0xf5);
pub const TEXT_STRONG: egui::Color32 = egui::Color32::from_rgb(0xea, 0xee, 0xfb);
pub const TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(0x7a, 0x82, 0xa3);
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(0x7a, 0xa2, 0xf7);
pub const ACCENT_STRONG: egui::Color32 = egui::Color32::from_rgb(0x9e, 0xc1, 0xff);
pub const SELECTION: egui::Color32 = egui::Color32::from_rgb(0x33, 0x4b, 0x7a);

pub const GET: egui::Color32 = egui::Color32::from_rgb(0x9e, 0xce, 0x6a);
pub const POST: egui::Color32 = egui::Color32::from_rgb(0x7a, 0xa2, 0xf7);
pub const PUT: egui::Color32 = egui::Color32::from_rgb(0xe0, 0xaf, 0x68);
pub const PATCH: egui::Color32 = egui::Color32::from_rgb(0xbb, 0x9a, 0xf7);
pub const DELETE: egui::Color32 = egui::Color32::from_rgb(0xf7, 0x76, 0x8e);
pub const METHOD_DEFAULT: egui::Color32 = egui::Color32::from_rgb(0x7a, 0x82, 0xa3);

pub const STATUS_2XX: egui::Color32 = egui::Color32::from_rgb(0x9e, 0xce, 0x6a);
pub const STATUS_3XX: egui::Color32 = egui::Color32::from_rgb(0xe0, 0xaf, 0x68);
pub const STATUS_4XX: egui::Color32 = egui::Color32::from_rgb(0xff, 0x9e, 0x64);
pub const STATUS_5XX: egui::Color32 = egui::Color32::from_rgb(0xf7, 0x76, 0x8e);
pub const STATUS_PENDING: egui::Color32 = egui::Color32::from_rgb(0x7a, 0x82, 0xa3);

pub const JSON_KEY: egui::Color32 = egui::Color32::from_rgb(0x7a, 0xa2, 0xf7);
pub const JSON_STRING: egui::Color32 = egui::Color32::from_rgb(0x9e, 0xce, 0x6a);
pub const JSON_NUMBER: egui::Color32 = egui::Color32::from_rgb(0xff, 0x9e, 0x64);
pub const JSON_BOOL: egui::Color32 = egui::Color32::from_rgb(0xbb, 0x9a, 0xf7);
pub const JSON_NULL: egui::Color32 = egui::Color32::from_rgb(0xf7, 0x76, 0x8e);
pub const JSON_PUNCT: egui::Color32 = egui::Color32::from_rgb(0x7a, 0x82, 0xa3);

pub fn install(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();

    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = BG;
    visuals.faint_bg_color = SURFACE;
    visuals.code_bg_color = SURFACE;
    visuals.selection.bg_fill = SELECTION;
    visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT);
    visuals.hyperlink_color = ACCENT_STRONG;
    visuals.window_stroke = egui::Stroke::new(1.0, BORDER);

    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.weak_bg_fill = PANEL;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT);

    visuals.widgets.inactive.bg_fill = SURFACE;
    visuals.widgets.inactive.weak_bg_fill = SURFACE;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT);

    visuals.widgets.hovered.bg_fill = SURFACE_HOVER;
    visuals.widgets.hovered.weak_bg_fill = SURFACE_HOVER;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, TEXT_STRONG);

    visuals.widgets.active.bg_fill = SELECTION;
    visuals.widgets.active.weak_bg_fill = SELECTION;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT_STRONG);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, TEXT_STRONG);

    visuals.widgets.open.bg_fill = SURFACE;
    visuals.widgets.open.weak_bg_fill = SURFACE;
    visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0, BORDER);
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, TEXT);

    ctx.set_visuals(visuals);

    let mut style = (*ctx.global_style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 4.0);
    style.spacing.window_margin = egui::Margin::same(10);
    style.spacing.indent = 18.0;

    use egui::{FontFamily, FontId, TextStyle};
    style.text_styles = [
        (TextStyle::Heading, FontId::new(16.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
    ]
    .into();

    ctx.set_global_style(style);
}

pub fn method_color(method: &str) -> egui::Color32 {
    match method.to_ascii_uppercase().as_str() {
        "GET" => GET,
        "POST" => POST,
        "PUT" => PUT,
        "PATCH" => PATCH,
        "DELETE" => DELETE,
        _ => METHOD_DEFAULT,
    }
}

pub fn status_color(status: Option<u16>) -> egui::Color32 {
    match status {
        Some(code) if (200..300).contains(&code) => STATUS_2XX,
        Some(code) if (300..400).contains(&code) => STATUS_3XX,
        Some(code) if (400..500).contains(&code) => STATUS_4XX,
        Some(_) => STATUS_5XX,
        None => STATUS_PENDING,
    }
}

pub fn method_badge(method: &str) -> egui::RichText {
    egui::RichText::new(format!(" {method} "))
        .monospace()
        .strong()
        .color(TEXT_STRONG)
        .background_color(method_color(method).gamma_multiply(0.35))
}

pub fn status_badge(status: Option<u16>) -> egui::RichText {
    let text = status
        .map(|code| code.to_string())
        .unwrap_or_else(|| "—".to_owned());
    egui::RichText::new(format!(" {text} "))
        .monospace()
        .strong()
        .color(TEXT_STRONG)
        .background_color(status_color(status).gamma_multiply(0.4))
}

pub fn classify_content_type(content_type: Option<&str>) -> BodyKind {
    let Some(ct) = content_type else {
        return BodyKind::Text;
    };
    let ct = ct.to_ascii_lowercase();
    if ct.contains("json") {
        BodyKind::Json
    } else if ct.contains("xml") || ct.contains("html") {
        BodyKind::Markup
    } else {
        BodyKind::Text
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    Json,
    Markup,
    Text,
}

pub fn pretty_print_json(text: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
}

pub fn json_layout_job(text: &str, font_size: f32, wrap_width: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_width;
    let font_id = egui::FontId::new(font_size, egui::FontFamily::Monospace);

    let bytes = text.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'"' => {
                let start = i;
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                let mut j = i;
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }
                let is_key = j < bytes.len() && bytes[j] == b':';
                let color = if is_key { JSON_KEY } else { JSON_STRING };
                append_slice(&mut job, text, start, i, &font_id, color);
            }
            b't' | b'f' if matches_kw(bytes, i, b"true") || matches_kw(bytes, i, b"false") => {
                let len = if matches_kw(bytes, i, b"true") { 4 } else { 5 };
                append_slice(&mut job, text, i, i + len, &font_id, JSON_BOOL);
                i += len;
            }
            b'n' if matches_kw(bytes, i, b"null") => {
                append_slice(&mut job, text, i, i + 4, &font_id, JSON_NULL);
                i += 4;
            }
            b'-' | b'0'..=b'9' => {
                let start = i;
                if bytes[i] == b'-' {
                    i += 1;
                }
                while i < bytes.len()
                    && (bytes[i].is_ascii_digit()
                        || bytes[i] == b'.'
                        || bytes[i] == b'e'
                        || bytes[i] == b'E'
                        || bytes[i] == b'+'
                        || bytes[i] == b'-')
                {
                    i += 1;
                }
                append_slice(&mut job, text, start, i, &font_id, JSON_NUMBER);
            }
            b'{' | b'}' | b'[' | b']' | b':' | b',' => {
                append_slice(&mut job, text, i, i + 1, &font_id, JSON_PUNCT);
                i += 1;
            }
            _ => {
                let start = i;
                while i < bytes.len() && !matches!(bytes[i], b'"' | b'{' | b'}' | b'[' | b']' | b':' | b',' | b'-' | b'0'..=b'9')
                    && !matches_kw(bytes, i, b"true")
                    && !matches_kw(bytes, i, b"false")
                    && !matches_kw(bytes, i, b"null")
                {
                    i += 1;
                }
                if i == start {
                    i += 1;
                }
                append_slice(&mut job, text, start, i, &font_id, TEXT);
            }
        }
    }

    job
}

fn matches_kw(bytes: &[u8], i: usize, kw: &[u8]) -> bool {
    bytes.len() >= i + kw.len() && &bytes[i..i + kw.len()] == kw
}

fn append_slice(
    job: &mut egui::text::LayoutJob,
    text: &str,
    start: usize,
    end: usize,
    font_id: &egui::FontId,
    color: egui::Color32,
) {
    if start >= end {
        return;
    }
    if let Some(slice) = text.get(start..end) {
        job.append(
            slice,
            0.0,
            egui::TextFormat::simple(font_id.clone(), color),
        );
    }
}
