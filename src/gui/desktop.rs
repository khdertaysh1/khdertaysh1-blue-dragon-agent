//! Native desktop shell over the graphical client's existing agent worker.
use super::{Client, Request};
use crate::{agent::ApprovalMode, config::Settings, tools::ToolBox, tui_text::visual_line};
use anyhow::{ensure, Context, Result};
use eframe::egui::{self, Color32, RichText};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const NAVY: Color32 = Color32::from_rgb(12, 22, 41);
const BLUE: Color32 = Color32::from_rgb(33, 93, 199);
const LINK: Color32 = Color32::from_rgb(133, 187, 255);

#[derive(Serialize, Deserialize)]
struct Bookmark {
    workspace: PathBuf,
    session: String,
}
impl Bookmark {
    fn save(&self, path: &Path) -> Result<()> {
        let mut file =
            tempfile::NamedTempFile::new_in(path.parent().context("Missing app directory")?)?;
        serde_json::to_writer(&mut file, self)?;
        file.persist(path)?;
        Ok(())
    }
}
pub(crate) fn run(
    mut settings: Settings,
    readonly: bool,
    mut resume: Option<String>,
    pinned: bool,
    new: bool,
) -> Result<()> {
    let bookmark = settings
        .sessions_dir
        .parent()
        .context("Missing app directory")?
        .join("desktop-session.json");
    if !new && resume.is_none() {
        if let Some(b) = fs::read(&bookmark)
            .ok()
            .and_then(|v| serde_json::from_slice::<Bookmark>(&v).ok())
        {
            if (!pinned || b.workspace == settings.root)
                && b.workspace.is_dir()
                && crate::session::load(&settings.sessions_dir, &b.session, &b.workspace).is_ok()
            {
                settings.root = b.workspace;
                resume = Some(b.session);
            }
        }
    }
    let client = super::start(settings, readonly, resume, true)?;
    let logo = image::load_from_memory_with_format(
        include_bytes!("../../assets/dragon-head.webp"),
        image::ImageFormat::WebP,
    )?
    .thumbnail(128, 128)
    .into_rgba8();
    let size = [logo.width() as usize, logo.height() as usize];
    let pixels = egui::ColorImage::from_rgba_unmultiplied(size, logo.as_raw());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(egui::IconData {
                width: logo.width(),
                height: logo.height(),
                rgba: logo.into_raw(),
            })
            .with_inner_size([1040., 730.])
            .with_min_inner_size([650., 480.]),
        ..Default::default()
    };
    eframe::run_native(
        "Blue Dragon",
        options,
        Box::new(move |cc| {
            let ctx = &cc.egui_ctx;
            let mut visuals = egui::Visuals::dark();
            visuals.panel_fill = NAVY;
            visuals.window_fill = Color32::from_rgb(18, 34, 58);
            visuals.hyperlink_color = LINK;
            visuals.selection.bg_fill = BLUE;
            ctx.set_visuals(visuals);
            let mut style = (*ctx.style()).clone();
            style
                .text_styles
                .insert(egui::TextStyle::Body, egui::FontId::proportional(17.));
            style
                .text_styles
                .insert(egui::TextStyle::Button, egui::FontId::proportional(16.));
            style.spacing.item_spacing = egui::vec2(10., 8.);
            ctx.set_style(style);
            let mut fonts = egui::FontDefinitions::default();
            // System Arabic fallback, using presentation forms supported by the shared bidi renderer.
            for path in [
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/System/Library/Fonts/Supplemental/Arial.ttf",
                "C:/Windows/Fonts/arial.ttf",
            ] {
                if let Ok(bytes) = fs::read(path) {
                    fonts
                        .font_data
                        .insert("Arabic".into(), egui::FontData::from_owned(bytes).into());
                    fonts
                        .families
                        .entry(egui::FontFamily::Proportional)
                        .or_default()
                        .push("Arabic".into());
                    ctx.set_fonts(fonts);
                    break;
                }
            }
            Ok(Box::new(Desktop {
                emblem: ctx.load_texture("Blue Dragon", pixels, Default::default()),
                client,
                bookmark,
                state: Value::Null,
                composer: String::new(),
                error: String::new(),
                folder: None,
                location: String::new(),
                sessions: None,
                source: None,
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("Unable to open native desktop window: {e}"))
}

struct Source {
    path: String,
    line: usize,
    rows: Vec<(usize, String)>,
}
impl Source {
    fn open(root: &str, path: &str, line: usize) -> Result<Self> {
        let files = ToolBox::new(PathBuf::from(root), ApprovalMode::ReadOnly);
        let full = files.local_file_path(path)?;
        ensure!(full.is_file(), "Choose a regular source file");
        let start = line.saturating_sub(5).max(1);
        use std::io::{BufRead, BufReader, Read};
        let mut reader = BufReader::new(fs::File::open(full)?.take(16 * 1024 * 1024));
        let mut row = String::new();
        let mut rows = Vec::new();
        for n in 1..=line.saturating_add(6) {
            row.clear();
            if reader.read_line(&mut row)? == 0 {
                break;
            }
            if n >= start {
                rows.push((
                    n,
                    row.trim_end_matches(['\r', '\n'])
                        .chars()
                        .take(1000)
                        .collect(),
                ));
            }
        }
        ensure!(
            rows.iter().any(|(n, _)| *n == line),
            "Line outside the readable source window"
        );
        Ok(Self {
            path: path.to_owned(),
            line,
            rows,
        })
    }
}
fn citations(text: &str) -> Vec<(&str, usize)> {
    let mut out = Vec::new();
    for (i, part) in text.split('`').enumerate() {
        fn parse(token: &str) -> Option<(&str, usize)> {
            let token = token
                .trim_start_matches(['(', '[', '"', '\''])
                .trim_end_matches(|c: char| {
                    matches!(c, '(' | ')' | '[' | ']' | ',' | '.' | ';' | '"' | '\'')
                });
            let (path, line) = token.rsplit_once(':')?;
            let n: usize = line.parse().ok()?;
            if n > 0
                && !path.is_empty()
                && !path.contains(':')
                && !path.starts_with(['/', '\\'])
                && !path.split(['/', '\\']).any(|part| part == "..")
                && !Path::new(path).is_absolute()
            {
                Some((path, n))
            } else {
                None
            }
        }
        if i % 2 == 1 {
            if let Some(item) = parse(part).filter(|item| !out.contains(item)) {
                out.push(item);
            }
        } else {
            for word in part.split_whitespace() {
                if let Some(item) = parse(word).filter(|item| !out.contains(item)) {
                    out.push(item);
                }
            }
        }
    }
    out
}

struct Desktop {
    emblem: egui::TextureHandle,
    client: Client,
    bookmark: PathBuf,
    state: Value,
    composer: String,
    error: String,
    folder: Option<PathBuf>,
    location: String,
    sessions: Option<Vec<Value>>,
    source: Option<Source>,
}
impl Desktop {
    fn request(&mut self, request: Request) -> bool {
        let starts_work = matches!(request, Request::Send { .. } | Request::Open { .. });
        match self.client.api(request) {
            Ok(_) => {
                if starts_work {
                    self.state["busy"] = true.into();
                }
                self.error.clear();
                true
            }
            Err(e) => {
                self.error = e.to_string();
                false
            }
        }
    }
    fn refresh(&mut self) {
        if let Ok(s) = self.client.api(Request::State {
            since: self.state["revision"].as_u64(),
        }) {
            if !s.is_null() {
                if s["session"] != self.state["session"] {
                    self.source = None;
                    self.composer.clear();
                    let b = Bookmark {
                        session: s["session"].as_str().unwrap_or_default().into(),
                        workspace: PathBuf::from(s["workspace"].as_str().unwrap_or_default()),
                    };
                    if let Err(e) = b.save(&self.bookmark) {
                        self.error = format!("Could not remember the session: {e}");
                    }
                }
                self.state = s;
            }
        }
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(dir) = self.folder.clone() {
            let mut visible = true;
            egui::Window::new("Open project")
                .open(&mut visible)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Up").clicked() {
                            if let Some(p) = dir.parent() {
                                self.location = p.display().to_string();
                                self.folder = Some(p.into());
                            }
                        }
                        ui.text_edit_singleline(&mut self.location);
                    });
                    if let Ok(entries) = fs::read_dir(&dir) {
                        let mut dirs: Vec<_> = entries
                            .flatten()
                            .map(|e| e.path())
                            .filter(|p| p.is_dir())
                            .collect();
                        dirs.sort();
                        egui::ScrollArea::vertical()
                            .max_height(235.)
                            .show(ui, |ui| {
                                for path in dirs {
                                    if ui
                                        .selectable_label(
                                            false,
                                            path.file_name().unwrap_or_default().to_string_lossy(),
                                        )
                                        .clicked()
                                    {
                                        self.location = path.display().to_string();
                                        self.folder = Some(path);
                                    }
                                }
                            });
                    }
                    if ui.button("Open this folder").clicked()
                        && self.request(Request::Open {
                            workspace: self.location.clone(),
                            session: None,
                        })
                    {
                        self.folder = None;
                    }
                });
            if !visible {
                self.folder = None;
            }
        }
        if let Some(rows) = self.sessions.take() {
            let mut visible = true;
            let mut picked = false;
            egui::Window::new("Saved sessions")
                .open(&mut visible)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(320.)
                        .show(ui, |ui| {
                            for s in &rows {
                                let path = s["workspace"].as_str().unwrap_or_default();
                                let id = s["id"].as_str().unwrap_or_default();
                                if ui.button(format!("{path}\n{id}")).clicked() {
                                    picked = self.request(Request::Open {
                                        workspace: path.into(),
                                        session: Some(id.into()),
                                    });
                                }
                            }
                        });
                });
            if visible && !picked {
                self.sessions = Some(rows);
            }
        }
        if let Some(source) = &self.source {
            let mut visible = true;
            egui::Window::new(format!("{}:{}", source.path, source.line))
                .open(&mut visible)
                .default_width(560.)
                .show(ctx, |ui| {
                    egui::ScrollArea::both().show(ui, |ui| {
                        for (n, row) in &source.rows {
                            let mut text = RichText::new(format!("{n:>5}   {row}")).monospace();
                            if *n == source.line {
                                text = text.background_color(BLUE).strong();
                            }
                            ui.label(text);
                        }
                    });
                });
            if !visible {
                self.source = None;
            }
        }
    }
}
impl eframe::App for Desktop {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.refresh();
        let root = self.state["workspace"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let busy = self.state["busy"].as_bool().unwrap_or(false);
        egui::TopBottomPanel::top("brand").show(ctx, |ui| {
            ui.add_space(7.);
            ui.horizontal(|ui| {
                ui.image((self.emblem.id(), egui::vec2(36., 36.)));
                ui.heading(RichText::new("Blue Dragon").color(LINK).strong());
                ui.separator();
                ui.label(self.state["model"].as_str().unwrap_or_default());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(!busy, |ui| {
                        if ui.button("Sessions").clicked() {
                            match self.client.api(Request::Sessions) {
                                Ok(v) => self.sessions = v.as_array().cloned(),
                                Err(e) => self.error = e.to_string(),
                            }
                        }
                        if ui.button("Open project").clicked() {
                            self.folder = Some(root.clone().into());
                            self.location = root.clone();
                        }
                    });
                });
            });
            ui.add_space(7.);
        });
        egui::TopBottomPanel::bottom("compose").show(ctx, |ui| {
            if !self.error.is_empty() {
                ui.colored_label(Color32::LIGHT_RED, &self.error);
            }
            let a = &self.state["approval"];
            if let Some(id) = a["id"].as_u64() {
                ui.strong(a["action"].as_str().unwrap_or_default());
                egui::ScrollArea::vertical()
                    .max_height(130.)
                    .show(ui, |ui| {
                        ui.monospace(a["detail"].as_str().unwrap_or_default())
                    });
                ui.horizontal(|ui| {
                    for (label, allow) in [("Approve", true), ("Reject", false)] {
                        if ui.button(label).clicked() {
                            self.request(Request::Approve { id, allow });
                        }
                    }
                });
            }
            ui.add_space(6.);
            let edit = ui.add(
                egui::TextEdit::multiline(&mut self.composer)
                    .hint_text("Ask Blue Dragon…")
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal(|ui| {
                let enabled = !busy && !self.composer.trim().is_empty();
                let send = ui
                    .add_enabled(enabled, egui::Button::new("Send").fill(BLUE))
                    .clicked()
                    || (enabled
                        && edit.has_focus()
                        && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter)));
                if send
                    && self.request(Request::Send {
                        session: self.state["session"].as_str().unwrap_or_default().into(),
                        text: self.composer.clone(),
                    })
                {
                    self.composer.clear();
                }
                if busy {
                    ui.spinner();
                    ui.label("Working");
                }
            });
            ui.add_space(5.);
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(
                Path::new(&root)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy(),
            );
            ui.weak(&root);
            ui.separator();
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if let Some(rows) = self.state["rows"].as_array() {
                        let mut i = 0;
                        while i < rows.len() {
                            let role = rows[i][0].as_str().unwrap_or_default();
                            if matches!(role, "Tool" | "Result") {
                                let first = i;
                                while i < rows.len()
                                    && matches!(rows[i][0].as_str(), Some("Tool" | "Result"))
                                {
                                    i += 1;
                                }
                                let steps =
                                    rows[first..i].iter().filter(|r| r[0] == "Tool").count();
                                egui::CollapsingHeader::new(format!("Tool activity · {steps}"))
                                    .id_salt((&self.state["session"].as_str(), first))
                                    .show(ui, |ui| {
                                        for r in &rows[first..i] {
                                            ui.monospace(r[1].as_str().unwrap_or_default());
                                        }
                                    });
                                continue;
                            }
                            let text = rows[i][1].as_str().unwrap_or_default();
                            ui.label(RichText::new(role).small().color(LINK));
                            ui.label(text.lines().map(visual_line).collect::<Vec<_>>().join("\n"));
                            if role == "Dragon" {
                                for (path, n) in citations(text) {
                                    if ui.link(format!("{path}:{n}")).clicked() {
                                        match Source::open(&root, path, n) {
                                            Ok(s) => self.source = Some(s),
                                            Err(e) => self.error = e.to_string(),
                                        }
                                    }
                                }
                            }
                            ui.add_space(5.);
                            i += 1;
                        }
                    }
                    if let Some(draft) = self.state["draft"].as_str().filter(|s| !s.is_empty()) {
                        ui.label(visual_line(draft));
                    }
                });
        });
        self.dialogs(ctx);
        if self.state["busy"] == true {
            ctx.request_repaint_after(Duration::from_millis(80));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_references_preserve_names_and_skip_external_paths() {
        assert_eq!(
            citations(
                "See `a folder/file.ts:16`, .env:2. ../private:3 https://host/file:4 /etc/file:1"
            ),
            vec![("a folder/file.ts", 16), (".env", 2)]
        );
    }

    #[test]
    fn source_preview_highlights_the_absolute_line_number() {
        let root = tempfile::tempdir().unwrap();
        let lines = (1..=25)
            .map(|n| format!("row {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(root.path().join("file.js"), lines).unwrap();
        let source = Source::open(root.path().to_str().unwrap(), "file.js", 16).unwrap();
        assert_eq!(source.rows[5], (16, "row 16".to_string()));
        assert!(Source::open(root.path().to_str().unwrap(), "file.js", 26).is_err());
    }
}
