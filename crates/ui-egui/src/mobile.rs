//! Touch chrome and two-finger navigation, shared by Android and the offscreen harness.
use crate::{
    PhotocraftApp,
    state::{Tool, View},
    theme::Tokens,
};
use egui::{Pos2, Rect, vec2};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sheet {
    Tools,
    Layers,
    Options,
    Color,
    History,
    Adjustments,
    Commands,
    Export,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MobileState {
    pub enabled: bool,
    pub sheet: Option<Sheet>,
    pub query: String,
    pub export_name: String,
    #[serde(skip)]
    contacts: Vec<Contact>,
    #[serde(skip)]
    navigating: bool,
    #[serde(skip)]
    release_guard: bool,
    #[serde(skip)]
    previous: Option<(Pos2, f32)>,
}
#[derive(Clone, Debug, PartialEq)]
struct Contact {
    device: u64,
    id: u64,
    pos: Pos2,
    origin: Pos2,
}

/// Track raw contacts instead of egui's synthetic mouse, including the last release frame.
pub fn record_touches(state: &mut MobileState, events: &[egui::Event]) {
    state.release_guard = false;
    if !state.enabled {
        return;
    }
    for event in events {
        let egui::Event::Touch { device_id, id, phase, pos, .. } = event else { continue };
        if !pos.x.is_finite() || !pos.y.is_finite() {
            continue;
        }
        let found = state.contacts.iter().position(|c| c.device == device_id.0 && c.id == id.0);
        match phase {
            egui::TouchPhase::Start | egui::TouchPhase::Move => {
                if let Some(i) = found {
                    if let Some(c) = state.contacts.get_mut(i) {
                        c.pos = *pos;
                    }
                } else if state.contacts.len() < 16 {
                    state.contacts.push(Contact { device: device_id.0, id: id.0, pos: *pos, origin: *pos });
                }
            }
            egui::TouchPhase::End | egui::TouchPhase::Cancel => {
                if let Some(i) = found {
                    state.contacts.remove(i);
                }
                if state.navigating && state.contacts.is_empty() {
                    state.release_guard = true;
                    state.navigating = false;
                    state.previous = None;
                }
            }
        }
    }
}

/// Keep the document point under the old centroid under the new centroid after scaling.
fn transform(view: &mut View, rect: Rect, old: Pos2, new: Pos2, factor: f32, flip: bool) {
    if !factor.is_finite() || factor <= 0.0 || !view.zoom.is_finite() || view.zoom <= 0.0 {
        return;
    }
    let sign = if flip { -1.0 } else { 1.0 };
    let before = old - rect.center();
    let after = new - rect.center();
    let next = (view.zoom * factor).clamp(0.01, 64.0);
    view.center[0] += sign * (before.x / view.zoom - after.x / next);
    view.center[1] += before.y / view.zoom - after.y / next;
    view.zoom = next;
}

/// Returns true while navigation owns this contact sequence (including a one-finger tail).
pub fn navigate(state: &mut MobileState, view: &mut View, rect: Rect, flip: bool) -> bool {
    if !state.enabled {
        return false;
    }
    let pair = state.contacts.first().zip(state.contacts.get(1));
    if let Some((a, b)) = pair {
        if !state.navigating && rect.contains(a.origin) && rect.contains(b.origin) {
            state.navigating = true;
        }
        if state.navigating {
            let center = a.pos + (b.pos - a.pos) * 0.5;
            let distance = a.pos.distance(b.pos).max(1.0);
            if let Some((old, span)) = state.previous {
                transform(view, rect, old, center, distance / span, flip);
            }
            state.previous = Some((center, distance));
        }
    } else {
        state.previous = None;
    }
    state.navigating || state.release_guard
}

fn invoke(app: &mut PhotocraftApp, ctx: &egui::Context, id: &str) {
    if let Err(e) = crate::menus::invoke(app, ctx, id, json!({})) {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}
fn action(app: &mut PhotocraftApp, ui: &mut egui::Ui, label: &str, id: &str) {
    let enabled = crate::menus::is_enabled(app, id);
    if ui.add_enabled(enabled, egui::Button::new(label).min_size(vec2(48.0, 48.0))).clicked() {
        invoke(app, ui.ctx(), id);
    }
}
fn toggle(app: &mut PhotocraftApp, sheet: Sheet) {
    app.ui.mobile.sheet = if app.ui.mobile.sheet == Some(sheet) { None } else { Some(sheet) };
}

pub fn chrome(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    // Scoped spacing keeps desktop styling intact when a mobile preview is disabled.
    egui::Panel::top("mobile-header").resizable(false).frame(egui::Frame::NONE.fill(t.chrome).inner_margin(4)).show(ui, |ui| {
        ui.spacing_mut().interact_size.y = 48.0;
        ui.horizontal(|ui| {
            if crate::icons::button(ui, "ellipsis", 48.0, app.ui.mobile.sheet == Some(Sheet::Commands), "Commands").clicked() {
                toggle(app, Sheet::Commands);
            }
            ui.label(egui::RichText::new("PhotoCraft").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new("Save").min_size(vec2(56.0, 48.0))).clicked() {
                    toggle(app, Sheet::Export);
                }
                for (icon, id, tip) in [("redo-2", "edit.redo", "Redo"), ("undo-2", "edit.undo", "Undo")] {
                    ui.add_enabled_ui(crate::menus::is_enabled(app, id), |ui| {
                        if crate::icons::button(ui, icon, 48.0, false, tip).clicked() {
                            invoke(app, ui.ctx(), id);
                        }
                    });
                }
            });
        });
    });
    egui::Panel::bottom("mobile-toolbar").resizable(false).frame(egui::Frame::NONE.fill(t.chrome).inner_margin(4)).show(ui, |ui| {
        ui.spacing_mut().interact_size.y = 48.0;
        let landscape = ui.ctx().content_rect().width() > ui.ctx().content_rect().height();
        egui::ScrollArea::horizontal().id_salt("mobile-tool-strip").show(ui, |ui| {
            ui.horizontal(|ui| {
                for tool in [Tool::Hand, Tool::Move, Tool::Brush, Tool::Eraser, Tool::RectMarquee, Tool::Crop, Tool::Type] {
                    if crate::icons::button(ui, crate::icons::tool_icon(tool), 48.0, app.ui.tool == tool, tool.label()).clicked() {
                        app.ui.tool = tool;
                    }
                }
                if landscape {
                    for (label, sheet) in [("Tools", Sheet::Tools), ("Layers", Sheet::Layers), ("Options", Sheet::Options), ("Color", Sheet::Color)] {
                        if ui.add(egui::Button::new(label).selected(app.ui.mobile.sheet == Some(sheet)).min_size(vec2(68.0, 48.0))).clicked() {
                            toggle(app, sheet);
                        }
                    }
                }
            });
        });
        if !landscape {
            ui.horizontal_wrapped(|ui| {
                for (label, sheet) in [("Tools", Sheet::Tools), ("Layers", Sheet::Layers), ("Options", Sheet::Options), ("Color", Sheet::Color)] {
                    if ui.add(egui::Button::new(label).selected(app.ui.mobile.sheet == Some(sheet)).min_size(vec2(68.0, 48.0))).clicked() {
                        toggle(app, sheet);
                    }
                }
            });
        }
        if !app.ui.status.is_empty() {
            let color = if app.ui.status_error { t.warning } else { t.text_faint };
            ui.add(egui::Label::new(egui::RichText::new(&app.ui.status).color(color)).truncate());
        }
    });
    if let Some(sheet) = app.ui.mobile.sheet {
        let screen = ui.ctx().content_rect();
        let frame = egui::Frame::NONE.fill(t.chrome).inner_margin(8);
        if screen.width() > screen.height() && screen.width() >= 600.0 {
            egui::Panel::right("mobile-drawer-side")
                .resizable(false)
                .exact_size((screen.width() * 0.4).clamp(240.0, 360.0))
                .frame(frame)
                .show(ui, |ui| sheet_content(app, ui, sheet));
        } else {
            let height = (screen.height() * 0.42).clamp(96.0, 420.0);
            egui::Panel::bottom("mobile-drawer").resizable(false).exact_size(height).frame(frame).show(ui, |ui| sheet_content(app, ui, sheet));
        }
    }
}
fn sheet_content(app: &mut PhotocraftApp, ui: &mut egui::Ui, sheet: Sheet) {
    ui.spacing_mut().interact_size.y = 48.0;
    ui.horizontal(|ui| {
        ui.strong(format!("{sheet:?}"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(egui::Button::new("Close").min_size(vec2(68.0, 48.0))).clicked() {
                app.ui.mobile.sheet = None;
            }
        });
    });
    egui::ScrollArea::both().id_salt(("mobile-sheet", sheet as u8)).show(ui, |ui| drawer(app, ui, sheet));
}

fn drawer(app: &mut PhotocraftApp, ui: &mut egui::Ui, sheet: Sheet) {
    match sheet {
        Sheet::Tools => {
            ui.horizontal_wrapped(|ui| {
                for &tool in &Tool::ALL {
                    if ui.add(egui::Button::new(tool.label()).selected(app.ui.tool == tool).min_size(vec2(92.0, 48.0))).clicked() {
                        app.ui.tool = tool;
                        app.ui.mobile.sheet = None;
                    }
                }
            })
            .inner
        }
        Sheet::Layers => layer_panel(app, ui),
        Sheet::Options => {
            ui.label(app.ui.tool.label());
            if app.ui.text_edit.is_some() && ui.button("Finish text").clicked() {
                crate::type_tool::commit(app);
            }
            if app.ui.transform.is_some() {
                ui.horizontal(|ui| {
                    if ui.button("Apply transform").clicked() {
                        crate::transform_tool::commit(app);
                    }
                    if ui.button("Cancel transform").clicked() {
                        crate::transform_tool::cancel(app);
                    }
                });
            }
            if app.ui.tool.is_brushlike() {
                ui.add(egui::Slider::new(&mut app.session.tools.brush.size, 1.0..=500.0).text("Size").logarithmic(true));
                ui.add(egui::Slider::new(&mut app.session.tools.brush.opacity, 0.0..=1.0).text("Opacity"));
            }
            if app.ui.tool == Tool::Crop && ui.button("Apply crop").clicked() {
                crate::canvas::commit_crop(app);
            }
            for (label, id) in [("Fit canvas", "view.fitOnScreen"), ("Deselect", "select.deselect")] {
                action(app, ui, label, id);
            }
            if ui.button("Adjustments").clicked() {
                app.ui.mobile.sheet = Some(Sheet::Adjustments);
            }
            if ui.button("History").clicked() {
                app.ui.mobile.sheet = Some(Sheet::History);
            }
        }
        Sheet::Commands => {
            ui.horizontal(|ui| {
                action(app, ui, "Open", "file.open");
                if ui.button("New 1080 × 1080").clicked() {
                    new_document(app);
                }
            });
            ui.add(egui::TextEdit::singleline(&mut app.ui.mobile.query).hint_text("Find a command…").desired_width(ui.available_width()));
            let items = crate::menus::menu_items(app);
            let query = app.ui.mobile.query.trim().to_lowercase();
            let mut found: Vec<_> = items
                .iter()
                .filter(|i| crate::menus::is_live(&i.id))
                .filter_map(|i| crate::menus::search_rank(&query, &i.label, &i.path).map(|rank| (rank, i)))
                .collect();
            found.sort_by_key(|(rank, _)| *rank);
            let found = found.into_iter().map(|(_, item)| item);
            for item in found.into_iter().filter(|i| crate::menus::is_live(&i.id)) {
                let label = format!("{} / {}", item.path.join(" / "), item.label);
                if ui.add_enabled(item.enabled, egui::Button::new(label).min_size(vec2(240.0, 48.0))).clicked() {
                    invoke(app, ui.ctx(), &item.id);
                    app.ui.mobile.sheet = None;
                }
            }
        }
        Sheet::Export => {
            ui.label("PSD and PhotoCraft keep layers. PNG and JPEG export the composite.");
            for (label, ext) in [("Save PSD", "psd"), ("Save PhotoCraft", "pcraft"), ("Export PNG", "png"), ("Export JPEG", "jpg")] {
                if ui.add_enabled(app.session.active().is_some(), egui::Button::new(label).min_size(vec2(200.0, 48.0))).clicked() {
                    let stem = app
                        .session
                        .active()
                        .map(|s| s.doc.name.rsplit_once('.').map_or(s.doc.name.as_str(), |(n, _)| n).to_string())
                        .unwrap_or_else(|| "Untitled".into());
                    app.ui.mobile.export_name = format!("{stem}.{ext}");
                    invoke(app, ui.ctx(), "file.saveAs");
                }
            }
        }
        _ => crate::panels::mobile_panel(app, ui, sheet),
    }
}
fn layer_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        for (label, id) in [("New", "layer.new.layer"), ("Duplicate", "layer.duplicate"), ("Delete", "layer.delete")] {
            action(app, ui, label, id);
        }
    });
    let Some(st) = app.session.active() else {
        ui.label("No document");
        return;
    };
    let doc = st.doc.clone();
    let active = st.active_layer;
    if let Some(layer) = active.and_then(|id| doc.layer(id)) {
        let mut opacity = layer.opacity;
        if ui.add(egui::Slider::new(&mut opacity, 0.0..=1.0).text("Opacity")).changed() {
            let _ = app.run("layer.setProps", json!({"layer": layer.id.0, "opacity": opacity}));
        }
        if layer.mask.is_some() {
            ui.checkbox(&mut app.ui.mask_target, "Paint on layer mask");
        }
    }
    let rows = doc.walk();
    for (_, depth, layer) in rows.into_iter().rev() {
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(if layer.visible { "Visible" } else { "Hidden" }).min_size(vec2(72.0, 48.0))).clicked() {
                let _ = app.run("layer.setProps", json!({"layer": layer.id.0, "visible": !layer.visible}));
            }
            let label = format!("{}{}", "  ".repeat(depth.min(8)), layer.name);
            if ui.add(egui::Button::new(label).selected(active == Some(layer.id)).min_size(vec2(168.0, 48.0))).clicked() {
                let _ = app.run("layer.select", json!({"layer": layer.id.0}));
                app.ui.mask_target =
                    layer.mask.is_some() && matches!(layer.content, photocraft_doc::LayerContent::Adjustment(_) | photocraft_doc::LayerContent::Fill(_));
                app.ui.vector_mask_target = false;
            }
        });
    }
}

fn new_document(app: &mut PhotocraftApp) {
    match app.run("file.new", json!({"width":1080,"height":1080,"name":"Untitled","background":"white"})) {
        Ok(_) => {
            app.sync_views();
            app.ui.chrome.home = None;
            app.ui.mobile.sheet = None;
        }
        Err(e) => {
            app.ui.status = e;
            app.ui.status_error = true;
        }
    }
}
pub fn welcome(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(24.0);
        ui.heading("PhotoCraft");
        ui.label("Open an image or create a canvas.");
        ui.add_space(16.0);
        action(app, ui, "Open image / PSD", "file.open");
        if ui.add(egui::Button::new("New 1080 × 1080 canvas").min_size(vec2(220.0, 48.0))).clicked() {
            new_document(app);
        }
        ui.add_space(16.0);
        ui.label("One finger: selected tool.\nTwo fingers: pan and pinch to zoom.");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn touch(id: u64, phase: egui::TouchPhase, x: f32, y: f32) -> egui::Event {
        egui::Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(id), phase, pos: egui::pos2(x, y), force: None }
    }
    #[test]
    fn pinch_keeps_anchor_and_pan_uses_document_coordinates() {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 600.0));
        let mut v = View { zoom: 2.0, center: [100.0, 100.0], ..Default::default() };
        transform(&mut v, rect, egui::pos2(240.0, 300.0), egui::pos2(260.0, 320.0), 2.0, false);
        assert_eq!(v.zoom, 4.0);
        assert_eq!(v.center, [105.0, 95.0]);
        let before = v.clone();
        transform(&mut v, rect, Pos2::ZERO, Pos2::ZERO, f32::NAN, false);
        assert_eq!(v, before);
    }
    #[test]
    fn navigation_owns_remaining_finger_and_final_release() {
        use egui::TouchPhase::*;
        let mut s = MobileState { enabled: true, ..Default::default() };
        let mut v = View::default();
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 600.0));
        record_touches(&mut s, &[touch(1, Start, 100.0, 100.0)]);
        assert!(!navigate(&mut s, &mut v, rect, false));
        record_touches(&mut s, &[touch(2, Start, 200.0, 100.0)]);
        assert!(navigate(&mut s, &mut v, rect, false));
        record_touches(&mut s, &[touch(2, End, 200.0, 100.0)]);
        assert!(navigate(&mut s, &mut v, rect, false));
        record_touches(&mut s, &[touch(1, End, 100.0, 100.0)]);
        assert!(navigate(&mut s, &mut v, rect, false));
        record_touches(&mut s, &[]);
        assert!(!navigate(&mut s, &mut v, rect, false));
    }
    #[test]
    fn contacts_starting_in_chrome_do_not_move_canvas() {
        let mut s = MobileState { enabled: true, ..Default::default() };
        let mut v = View::default();
        record_touches(&mut s, &[touch(1, egui::TouchPhase::Start, 100.0, 20.0), touch(2, egui::TouchPhase::Start, 200.0, 200.0)]);
        assert!(!navigate(&mut s, &mut v, Rect::from_min_max(egui::pos2(0.0, 80.0), egui::pos2(400.0, 600.0)), false));
    }
}

#[cfg(test)]
mod save_tests {
    use super::*;
    use crate::{SaveResults, Services};
    use std::sync::{Arc, Mutex};
    #[test]
    fn async_save_waits_for_picker_and_cancellation_keeps_document_dirty() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let queue: SaveResults = Arc::default();
        let capture = requests.clone();
        let mut app = PhotocraftApp::new(
            photocraft_engine::Session::new(),
            Services {
                pick_save_async: Some(Box::new(move |id, name| {
                    capture.lock().unwrap().push((id, name.to_string()));
                    Ok(())
                })),
                save_results: Some(queue.clone()),
                ..Default::default()
            },
        );
        app.run("file.new", json!({"width":16,"height":16})).unwrap();
        app.run("layer.new.layer", json!({"name":"Edited"})).unwrap();
        app.ui.mobile.enabled = true;
        app.ui.mobile.export_name = "image.png".into();
        let id = app.session.active().unwrap().doc.id;
        app.save_as(None).unwrap();
        assert_eq!(requests.lock().unwrap()[0], (id, "image.png".into()));
        assert!(app.session.active().unwrap().is_dirty());
        queue.lock().unwrap().push((id, Ok(None)));
        app.drain_save_results();
        assert!(app.session.active().unwrap().is_dirty());
        assert!(app.session.active().unwrap().path.is_none());
    }
    #[test]
    fn completion_saves_original_tab_and_failed_write_preserves_dirty_state() {
        let queue: SaveResults = Arc::default();
        let writes = Arc::new(Mutex::new(Vec::new()));
        let capture = writes.clone();
        let mut app = PhotocraftApp::new(
            photocraft_engine::Session::new(),
            Services {
                save_results: Some(queue.clone()),
                export: Some(Box::new(|doc, _, _| Ok((doc.name.as_bytes().to_vec(), vec![])))),
                write: Some(Box::new(move |path, bytes| {
                    capture.lock().unwrap().push((path.to_string(), bytes.to_vec()));
                    Ok(())
                })),
                ..Default::default()
            },
        );
        app.run("file.new", json!({"width":16,"height":16,"name":"First"})).unwrap();
        app.run("layer.new.layer", json!({"name":"Edited"})).unwrap();
        let first = app.session.active().unwrap().doc.id;
        app.run("file.new", json!({"width":16,"height":16,"name":"Second"})).unwrap();
        let second = app.session.active().unwrap().doc.id;
        queue.lock().unwrap().push((first, Ok(Some("content://test/document#First.psd".into()))));
        app.drain_save_results();
        assert_eq!(app.session.active().unwrap().doc.id, second);
        assert_eq!(writes.lock().unwrap()[0].1, b"First");
        assert!(!app.session.documents()[0].is_dirty());
        app.run("layer.new.layer", json!({"name":"Edited second"})).unwrap();
        app.services.write = Some(Box::new(|_, _| Err("Provider unavailable".into())));
        queue.lock().unwrap().push((second, Ok(Some("content://test/document#Second.psd".into()))));
        app.drain_save_results();
        assert!(app.session.active().unwrap().is_dirty());
        assert!(app.ui.status_error);
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    #[test]
    fn phone_and_landscape_keep_a_usable_canvas_with_layers_open() {
        for size in [vec2(360.0, 800.0), vec2(800.0, 360.0)] {
            let mut h = egui_kittest::Harness::builder().with_size(size).with_max_steps(64).build_eframe(|cc| {
                PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
                app.ui.mobile.enabled = true;
                app.ui.mobile.sheet = Some(Sheet::Layers);
                app.run("file.new", json!({"width":32,"height":32})).unwrap();
                app
            });
            h.run_steps(5);
            let canvas = h.state().last_canvas_rect;
            assert!(canvas.width() >= if size.x > size.y { size.x * 0.5 } else { size.x - 32.0 }, "canvas width: {canvas:?}");
            assert!(canvas.height() >= 72.0, "canvas height: {canvas:?}");
            assert!(canvas.max.y <= size.y, "canvas below screen: {canvas:?}");
        }
    }
}
