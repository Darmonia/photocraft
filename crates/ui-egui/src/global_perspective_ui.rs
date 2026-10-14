//! Global Perspective Warp V3. Source quads are guides; the WHOLE image follows one matrix.
//! Source editing and result inspection are separate: no independent destination handles.

use crate::PhotocraftApp;
use crate::canvas::{ToolEvent, ViewXform};
use egui::{Color32, Pos2, Stroke, TextureHandle, pos2};
use photocraft_algo::global_perspective::{Guide, MAX_GUIDES, Options, Solution, solve, validate_guides, validate_options};
use photocraft_doc::{Document, LayerId};
use photocraft_engine::global_perspective_cmds::{COMMAND, validate_target};
use photocraft_geom::Rect;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

const GRID: usize = 128;
#[derive(Clone)]
enum Drag {
    New { start: [f64; 2], current: [f64; 2] },
    Corner { id: u64, corner: usize, offset: [f64; 2] },
}

pub struct GlobalSession {
    pub key: u64,
    pub layer: LayerId,
    pub preview_doc: Arc<Document>,
    source_doc: Arc<Document>,
    bounds: Rect,
    guides: Vec<Guide>,
    options: Options,
    selected: Option<u64>,
    solution: Option<Solution>,
    show_result: bool,
    drag: Option<Drag>,
    texture: TextureHandle,
    opacity: f32,
    grid: Vec<[f64; 2]>,
}
impl GlobalSession {
    pub fn describe(&self) -> Value {
        json!({"version":"V3","document":self.source_doc.id.0,"layer":self.layer.0,
            "mode":if self.show_result {"result"}else{"layout"},"guides":self.guides,
            "selectedGuide":self.selected,"options":self.options,"solved":self.solution.is_some(),
            "dirty":self.solution.is_none(),"solution":self.solution,"scope":"wholeRasterLayer"})
    }
    fn map(&self, p: [f64; 2]) -> [f64; 2] {
        if self.show_result
            && let Some(solution) = &self.solution
        {
            let (x, y) = solution.homography().apply(p[0], p[1]);
            [x, y]
        } else {
            p
        }
    }
    fn update_grid(&mut self) {
        let b = self.bounds;
        let mut grid = Vec::with_capacity((GRID + 1) * (GRID + 1));
        for y in 0..=GRID {
            for x in 0..=GRID {
                grid.push(self.map([b.x0 as f64 + b.width() as f64 * x as f64 / GRID as f64, b.y0 as f64 + b.height() as f64 * y as f64 / GRID as f64]));
            }
        }
        self.grid = grid;
    }
    fn invalidate(&mut self) {
        self.solution = None;
        self.show_result = false;
        self.update_grid();
    }
    fn move_corner(&mut self, id: u64, index: usize, p: [f64; 2]) {
        let p = [p[0].clamp(self.bounds.x0 as f64, self.bounds.x1 as f64), p[1].clamp(self.bounds.y0 as f64, self.bounds.y1 as f64)];
        let Some(guide) = self.guides.iter().find(|g| g.id == id) else { return };
        let mut next = guide.clone();
        let Some(c) = next.corners.get_mut(index) else { return };
        *c = p;
        if validate_guides(std::slice::from_ref(&next)).is_ok() {
            if let Some(guide) = self.guides.iter_mut().find(|g| g.id == id) {
                *guide = next;
            }
            self.invalidate();
        }
    }
}

pub fn bound_to_active_document(app: &PhotocraftApp, s: &GlobalSession) -> bool {
    app.session.active().is_some_and(|st| st.doc.id == s.source_doc.id)
}
fn validate_session(app: &PhotocraftApp) -> Result<(), String> {
    let s = app.distort.global_perspective.as_ref().ok_or("Global Perspective Warp is not active")?;
    let st = app.session.active().ok_or("no document open")?;
    if st.doc.id != s.source_doc.id {
        return Err("Global Perspective Warp belongs to another document".into());
    }
    if !Arc::ptr_eq(&st.doc, &s.source_doc) {
        return Err("The source document changed; cancel and reopen Global Perspective Warp to avoid a stale commit".into());
    }
    validate_target(&app.session, s.layer)?;
    Ok(())
}

pub fn begin(app: &mut PhotocraftApp, ctx: &egui::Context) -> Result<(), String> {
    if app.ui.transform.is_some() || app.distort.active() || app.distort.gallery.is_some() {
        return Err("finish or cancel the current transformation first".into());
    }
    let (layer, surface, _) = crate::distort_ui::active_pixels(app)?;
    let bounds = validate_target(&app.session, layer)?;
    let st = app.session.active().ok_or("no document")?;
    let l = st.doc.layer(layer).ok_or("no layer")?;
    let source_doc = st.doc.clone();
    let preview_doc = crate::distort_ui::without_layer(&st.doc, layer);
    let opacity = l.opacity * l.fill_opacity;
    let key = app.ui.alloc_id();
    let texture =
        ctx.load_texture(format!("global-perspective-v3-{key}"), crate::distort_ui::surface_image(&surface, bounds, 2048), egui::TextureOptions::LINEAR);
    let mut s = GlobalSession {
        key,
        layer,
        source_doc,
        preview_doc,
        bounds,
        guides: Vec::new(),
        options: Options::default(),
        selected: None,
        solution: None,
        show_result: false,
        drag: None,
        texture,
        opacity,
        grid: Vec::new(),
    };
    s.update_grid();
    app.distort.global_perspective = Some(s);
    app.ui.status = tl!("Global V3: draw independent guides on doors and windows, then rectify the whole photo").into();
    Ok(())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Control {
    guides: Option<Vec<Guide>>,
    options: Option<Options>,
    mode: Option<String>,
    selected_guide: Option<u64>,
    #[serde(default)]
    solve: bool,
    #[serde(default)]
    commit: bool,
    #[serde(default)]
    cancel: bool,
}

pub fn control(app: &mut PhotocraftApp, value: &Value) -> Result<Value, String> {
    if value.get("guides").and_then(Value::as_array).is_some_and(|a| a.len() > MAX_GUIDES) {
        return Err("too many guides (maximum 128)".into());
    }
    let p: Control = serde_json::from_value(value.clone()).map_err(|e| format!("invalid global perspective UI parameters: {e}"))?;
    if (p.commit || p.cancel) && value.as_object().is_none_or(|v| v.len() != 1) {
        return Err("commit or cancel must be a standalone action".into());
    }
    if p.cancel {
        app.distort.global_perspective = None;
        return Ok(json!({"cancelled":true}));
    }
    validate_session(app)?;
    if p.commit {
        return commit(app);
    }
    if p.mode.as_ref().is_some_and(|m| m != "layout" && m != "result") {
        return Err("mode must be layout|result".into());
    }
    let s = app.distort.global_perspective.as_mut().ok_or("Global Perspective Warp is not active")?;
    let replaced = p.guides.is_some() || p.options.is_some();
    let guides = p.guides.unwrap_or_else(|| s.guides.clone());
    let options = p.options.unwrap_or_else(|| s.options.clone());
    validate_guides(&guides)?;
    validate_options(&options)?;
    let selected = match p.selected_guide {
        Some(id) if guides.iter().any(|g| g.id == id) => Some(id),
        Some(_) => return Err("selectedGuide does not exist".into()),
        None => s.selected.filter(|id| guides.iter().any(|g| g.id == *id)).or_else(|| guides.last().map(|g| g.id)),
    };
    let should_solve = p.solve || p.mode.as_deref() == Some("result");
    // Solve and validate all candidate data before mutating the existing session.
    let solution = if should_solve {
        Some(solve(&guides, s.bounds, &options)?)
    } else if replaced {
        None
    } else {
        s.solution.clone()
    };
    let show_result = if p.mode.as_deref() == Some("layout") {
        false
    } else if should_solve {
        true
    } else if replaced {
        false
    } else {
        s.show_result
    };
    s.guides = guides;
    s.options = options;
    s.selected = selected;
    s.solution = solution;
    s.show_result = show_result;
    s.drag = None;
    s.update_grid();
    if let Some(sol) = &s.solution {
        app.ui.status = format!(
            "V3: {} / {} | {:.2} deg | {}",
            sol.used_guide_ids.len(),
            s.guides.iter().filter(|g| g.enabled).count(),
            sol.max_used_error_deg,
            tl!("One global transform; guides are not image regions")
        );
        if !sol.rejected_guide_ids.is_empty() {
            app.ui.status.push_str(&format!(" | {}: {:?}", tl!("Inconsistent guides"), sol.rejected_guide_ids));
        }
    }
    Ok(s.describe())
}

pub fn commit(app: &mut PhotocraftApp) -> Result<Value, String> {
    validate_session(app)?;
    let s = app.distort.global_perspective.take().ok_or("Global Perspective Warp is not active")?;
    if s.solution.is_none() {
        app.distort.global_perspective = Some(s);
        return Err("Rectify globally before committing; the guides have no current solution".into());
    }
    let p = json!({"document":s.source_doc.id.0,"layer":s.layer.0,"guides":s.guides,"options":s.options});
    match app.run(COMMAND, p) {
        Ok(result) => Ok(json!({"committed":result["changed"]==true,"result":result})),
        Err(error) => {
            app.distort.global_perspective = Some(s);
            app.ui.status = error.clone();
            Err(error)
        }
    }
}

fn inside(p: [f64; 2], q: &[[f64; 2]; 4]) -> bool {
    (0..4).all(|i| {
        let a = q[i];
        let b = q[(i + 1) % 4];
        (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]) >= 0.0
    })
}
pub fn pointer(app: &mut PhotocraftApp, ev: ToolEvent, mods: egui::Modifiers) {
    if let Err(e) = validate_session(app) {
        app.ui.status = e;
        return;
    }
    let tol = crate::distort_ui::tolerance(app);
    let Some(s) = app.distort.global_perspective.as_mut() else { return };
    match ev {
        ToolEvent::Down { x, y, .. } => {
            let p = [x, y];
            let mut corner = None;
            for g in s.guides.iter().rev() {
                for (i, c) in g.corners.iter().enumerate() {
                    let c = s.map(*c);
                    if (c[0] - x).hypot(c[1] - y) <= tol {
                        corner = Some((g.id, i, c));
                        break;
                    }
                }
                if corner.is_some() {
                    break;
                }
            }
            if let Some((id, i, c)) = corner {
                s.selected = Some(id);
                if !s.show_result {
                    s.drag = Some(Drag::Corner { id, corner: i, offset: [c[0] - x, c[1] - y] });
                }
            } else if !mods.shift
                && let Some(g) = s.guides.iter().rev().find(|g| inside(p, &g.corners.map(|c| s.map(c))))
            {
                s.selected = Some(g.id);
            } else if !s.show_result
                && s.guides.len() < MAX_GUIDES
                && x >= s.bounds.x0 as f64
                && x <= s.bounds.x1 as f64
                && y >= s.bounds.y0 as f64
                && y <= s.bounds.y1 as f64
            {
                s.drag = Some(Drag::New { start: p, current: p });
            }
        }
        ToolEvent::Move { x, y, .. } => match s.drag.clone() {
            Some(Drag::New { start, .. }) => s.drag = Some(Drag::New { start, current: [x, y] }),
            Some(Drag::Corner { id, corner, offset }) => s.move_corner(id, corner, [x + offset[0], y + offset[1]]),
            None => {}
        },
        ToolEvent::Up { x, y } => match s.drag.take() {
            Some(Drag::New { start, .. }) => {
                let end = [x.clamp(s.bounds.x0 as f64, s.bounds.x1 as f64), y.clamp(s.bounds.y0 as f64, s.bounds.y1 as f64)];
                let (x0, y0, x1, y1) = (start[0].min(end[0]), start[1].min(end[1]), start[0].max(end[0]), start[1].max(end[1]));
                if x1 - x0 > 4.0 && y1 - y0 > 4.0 && s.guides.len() < MAX_GUIDES {
                    let Some(id) = (1..=(MAX_GUIDES as u64 + 1)).find(|id| s.guides.iter().all(|g| g.id != *id)) else { return };
                    s.guides.push(Guide { id, corners: [[x0, y0], [x1, y0], [x1, y1], [x0, y1]], enabled: true, weight: 1.0 });
                    s.selected = Some(id);
                    s.invalidate();
                }
            }
            Some(Drag::Corner { id, corner, offset }) => s.move_corner(id, corner, [x + offset[0], y + offset[1]]),
            None => {}
        },
    }
}

pub fn draw(s: &GlobalSession, painter: &egui::Painter, xf: &ViewXform) {
    let scr = |p: [f64; 2]| xf.to_screen(p[0] as f32, p[1] as f32);
    let mut mesh = egui::Mesh::with_texture(s.texture.id());
    // Neutral image modulation, not a hard-coded UI theme color.
    let tint = Color32::from_white_alpha((s.opacity.clamp(0.0, 1.0) * 255.0) as u8);
    for (j, row) in s.grid.as_chunks::<{ GRID + 1 }>().0.iter().enumerate() {
        for (i, p) in row.iter().enumerate() {
            mesh.vertices.push(egui::epaint::Vertex { pos: scr(*p), uv: pos2(i as f32 / GRID as f32, j as f32 / GRID as f32), color: tint });
        }
    }
    for j in 0..GRID {
        for i in 0..GRID {
            let a = (j * (GRID + 1) + i) as u32;
            mesh.add_triangle(a, a + 1, a + GRID as u32 + 2);
            mesh.add_triangle(a, a + GRID as u32 + 2, a + GRID as u32 + 1);
        }
    }
    painter.add(mesh);
    let t = crate::theme::Tokens::get(painter.ctx());
    for g in &s.guides {
        let rejected = s.solution.as_ref().is_some_and(|sol| sol.rejected_guide_ids.contains(&g.id));
        let color = if !g.enabled {
            t.text_faint
        } else if rejected {
            t.warning
        } else {
            t.accent
        };
        let pts: Vec<Pos2> = g.corners.iter().map(|p| scr(s.map(*p))).collect();
        painter.add(egui::Shape::closed_line(pts.clone(), Stroke::new(if s.selected == Some(g.id) { 2.5 } else { 1.3 }, color)));
        for p in &pts {
            painter.circle_filled(*p, 4.5, t.card);
            painter.circle_stroke(*p, 4.5, Stroke::new(1.4, color));
        }
        if let Some(p) = pts.first() {
            painter.text(*p + egui::vec2(6.0, 6.0), egui::Align2::LEFT_TOP, g.id.to_string(), egui::FontId::monospace(12.0), color);
        }
    }
    if let Some(Drag::New { start, current }) = &s.drag {
        painter.rect_stroke(egui::Rect::from_two_pos(scr(*start), scr(*current)), 0.0, Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
    }
}

pub fn options_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let mut do_solve = false;
    let mut do_commit = false;
    let mut do_cancel = false;
    let Some(s) = app.distort.global_perspective.as_mut() else { return };
    ui.label("V3 Global");
    if ui.selectable_label(!s.show_result, tl!("Edit guides")).clicked() {
        s.show_result = false;
        s.update_grid();
    }
    if ui.button(tl!("Rectify globally")).clicked() {
        do_solve = true;
    }
    egui::ComboBox::from_id_salt("global_v3_selected").width(75.0).selected_text(s.selected.map_or_else(|| "-".into(), |id| format!("#{id}"))).show_ui(
        ui,
        |ui| {
            for g in &s.guides {
                ui.selectable_value(&mut s.selected, Some(g.id), format!("#{}", g.id));
            }
        },
    );
    let mut changed = false;
    if let Some(g) = s.guides.iter_mut().find(|g| Some(g.id) == s.selected) {
        changed = ui.checkbox(&mut g.enabled, tl!("Enabled")).changed();
    }
    if ui.button(tl!("Delete guide")).clicked() {
        s.guides.retain(|g| Some(g.id) != s.selected);
        s.selected = s.guides.last().map(|g| g.id);
        changed = true;
        s.drag = None;
    }
    ui.menu_button(tl!("Options"), |ui| {
        changed |= ui.checkbox(&mut s.options.fit_canvas, tl!("Fit whole photo")).changed();
        ui.horizontal(|ui| {
            ui.label(tl!("Global aspect"));
            changed |= ui.add(egui::DragValue::new(&mut s.options.aspect).range(0.25..=4.0).speed(0.01)).changed();
        });
        if let Some(g) = s.guides.iter_mut().find(|g| Some(g.id) == s.selected) {
            ui.horizontal(|ui| {
                ui.label(tl!("Guide weight"));
                changed |= ui.add(egui::DragValue::new(&mut g.weight).range(0.05..=20.0).speed(0.05)).changed();
            });
        }
        ui.separator();
        ui.label(tl!("One global transform; guides are not image regions"));
        ui.label(tl!("True proportions require a known measurement"));
        if let Some(solution) = &s.solution {
            ui.label(format!("{} / {} | {:.2} deg", solution.used_guide_ids.len(), s.guides.iter().filter(|g| g.enabled).count(), solution.max_used_error_deg));
            if !solution.rejected_guide_ids.is_empty() {
                ui.colored_label(crate::theme::Tokens::get(ui.ctx()).warning, format!("{}: {:?}", tl!("Inconsistent guides"), solution.rejected_guide_ids));
            }
        }
    });
    if changed {
        s.invalidate();
    }
    if let Some(sol) = &s.solution {
        ui.label(format!("{}/{}", sol.used_guide_ids.len(), s.guides.iter().filter(|g| g.enabled).count())).on_hover_text(tl!("Compatible guides"));
    }
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        do_commit = crate::widgets::primary_button(ui, "✓", 32.0).on_hover_text(tl!("Commit global rectification")).clicked();
        do_cancel = crate::widgets::secondary_button(ui, "⊘", 32.0).on_hover_text(tl!("Cancel (Esc)")).clicked();
    });
    if do_cancel {
        app.distort.global_perspective = None;
    } else if do_commit {
        if let Err(e) = commit(app) {
            app.ui.status = e;
        }
    } else if do_solve && let Err(e) = control(app, &json!({"solve":true})) {
        app.ui.status = e;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> PhotocraftApp {
        let mut a = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        a.session.execute("file.new", json!({"width":160,"height":140,"depth":8})).unwrap();
        a.session.execute("layer.new.layer", json!({})).unwrap();
        a.session
            .edit("fixture", |doc, id| {
                doc.layer_mut(id.unwrap()).unwrap().surface_mut().unwrap().fill_rect(Rect::new(0, 0, 160, 140), &[0.2, 0.5, 0.8, 1.0]);
                Ok(())
            })
            .unwrap();
        a.sync_views();
        begin(&mut a, &egui::Context::default()).unwrap();
        a
    }
    fn guides() -> Value {
        json!([{"id":1,"corners":[[25,20],[100,16],[109,105],[21,110]]}])
    }
    #[test]
    fn ai_solve_preview_commit_share_the_exact_matrix() {
        let mut a = app();
        let n = a.session.active().unwrap().history.past_len();
        let r = control(&mut a, &json!({"guides":guides(),"solve":true})).unwrap();
        assert_eq!(r["solved"], true);
        let out = commit(&mut a).unwrap();
        assert_eq!(out["committed"], true);
        assert_eq!(r["solution"]["matrix"], out["result"]["solution"]["matrix"]);
        assert_eq!(a.session.active().unwrap().history.past_len(), n + 1);
        assert!(a.distort.global_perspective.is_none());
    }
    #[test]
    fn malformed_controls_leave_preview_unchanged() {
        let mut a = app();
        let before = control(&mut a, &json!({"guides":guides(),"solve":true})).unwrap();
        for p in [
            json!({"typo":true}),
            json!({"solve":"yes"}),
            json!({"mode":"warp"}),
            json!({"selectedGuide":999}),
            json!({"options":{"aspect":0}}),
            json!({"commit":true,"guides":[]}),
        ] {
            assert!(control(&mut a, &p).is_err());
            assert_eq!(a.distort.global_perspective.as_ref().unwrap().describe(), before);
        }
    }
    #[test]
    fn changed_guides_invalidate_solution_and_forbid_stale_commit() {
        let mut a = app();
        control(&mut a, &json!({"guides":guides(),"solve":true})).unwrap();
        control(&mut a, &json!({"guides":guides()})).unwrap();
        assert!(commit(&mut a).is_err());
        assert!(a.distort.global_perspective.is_some());
    }
    #[test]
    fn cross_document_and_source_edits_are_rejected() {
        let mut a = app();
        control(&mut a, &json!({"guides":guides(),"solve":true})).unwrap();
        a.session.execute("file.new", json!({"width":20,"height":20})).unwrap();
        assert!(commit(&mut a).is_err());
        assert!(a.distort.global_perspective.is_some());
        let mut a = app();
        control(&mut a, &json!({"guides":guides(),"solve":true})).unwrap();
        a.session.execute("layer.new.layer", json!({})).unwrap();
        assert!(commit(&mut a).is_err());
    }
    #[test]
    fn cancellation_preserves_pixels_and_history() {
        let mut a = app();
        let before = a.session.active().unwrap().doc.clone();
        let n = a.session.active().unwrap().history.past_len();
        control(&mut a, &json!({"guides":guides(),"solve":true})).unwrap();
        control(&mut a, &json!({"cancel":true})).unwrap();
        assert!(Arc::ptr_eq(&before, &a.session.active().unwrap().doc));
        assert_eq!(a.session.active().unwrap().history.past_len(), n);
    }
    #[test]
    fn source_mouse_guides_are_independent_and_survive_list_replacement_during_drag() {
        let mut a = app();
        let mods = egui::Modifiers::NONE;
        pointer(&mut a, ToolEvent::Down { x: 10.0, y: 10.0, pressure: 1.0 }, mods);
        pointer(&mut a, ToolEvent::Up { x: 50.0, y: 60.0 }, mods);
        pointer(&mut a, ToolEvent::Down { x: 90.0, y: 80.0, pressure: 1.0 }, mods);
        pointer(&mut a, ToolEvent::Up { x: 140.0, y: 130.0 }, mods);
        assert_eq!(a.distort.global_perspective.as_ref().unwrap().guides.len(), 2);
        pointer(&mut a, ToolEvent::Down { x: 10.0, y: 10.0, pressure: 1.0 }, mods);
        control(&mut a, &json!({"guides":[]})).unwrap();
        pointer(&mut a, ToolEvent::Up { x: 30.0, y: 30.0 }, mods);
        assert!(a.distort.global_perspective.as_ref().unwrap().guides.is_empty());
    }
    #[test]
    fn menu_route_opens_and_controls_global_tool() {
        let mut a = app();
        a.distort.global_perspective = None;
        let ctx = egui::Context::default();
        crate::distort_ui::menu(&mut a, &ctx, COMMAND, &json!({})).unwrap().unwrap();
        let r = crate::distort_ui::menu(&mut a, &ctx, COMMAND, &json!({"ui":{"guides":guides(),"solve":true}})).unwrap().unwrap();
        assert_eq!(r["solved"], true);
    }

    #[test]
    fn result_handles_cannot_create_local_warps() {
        let mut a = app();
        let before = control(&mut a, &json!({"guides":guides(),"solve":true})).unwrap();
        let point = before["solution"]["guides"][0]["transformedCorners"][0].as_array().unwrap();
        let x = point[0].as_f64().unwrap();
        let y = point[1].as_f64().unwrap();
        pointer(&mut a, ToolEvent::Down { x, y, pressure: 1.0 }, egui::Modifiers::NONE);
        pointer(&mut a, ToolEvent::Move { x: x + 10.0, y: y + 10.0, pressure: 1.0 }, egui::Modifiers::NONE);
        pointer(&mut a, ToolEvent::Up { x: x + 10.0, y: y + 10.0 }, egui::Modifiers::NONE);
        let after = a.distort.global_perspective.as_ref().unwrap().describe();
        assert_eq!(after["guides"], before["guides"]);
        assert_eq!(after["solution"]["matrix"], before["solution"]["matrix"]);
    }
    #[test]
    fn repeated_begin_does_not_replace_an_editable_session() {
        let mut a = app();
        let before = control(&mut a, &json!({"guides":guides()})).unwrap();
        assert!(begin(&mut a, &egui::Context::default()).is_err());
        assert_eq!(a.distort.global_perspective.as_ref().unwrap().describe(), before);
    }
    #[test]
    fn selecting_a_guide_preserves_solved_geometry() {
        let mut a = app();
        let before = control(&mut a, &json!({"guides":guides(),"solve":true})).unwrap();
        let after = control(&mut a, &json!({"selectedGuide":1})).unwrap();
        assert_eq!(before["solution"], after["solution"]);
        assert_eq!(after["dirty"], false);
    }
    #[test]
    fn max_integer_guide_id_does_not_break_new_mouse_guides() {
        let mut a = app();
        let mut g = guides();
        g[0]["id"] = json!(u64::MAX);
        control(&mut a, &json!({"guides":g})).unwrap();
        pointer(&mut a, ToolEvent::Down { x: 120.0, y: 10.0, pressure: 1.0 }, egui::Modifiers::NONE);
        pointer(&mut a, ToolEvent::Up { x: 150.0, y: 70.0 }, egui::Modifiers::NONE);
        assert_eq!(a.distort.global_perspective.as_ref().unwrap().guides.len(), 2);
    }
}
