//! PR #3 follow-up regressions against parent 313436003638dfb1d5920cba4b1d02d804dc58a1.
use super::*;

fn app(ctx: &egui::Context) -> PhotocraftApp {
    let mut a = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    a.session.execute("file.new", json!({"width":160,"height":140,"depth":8})).unwrap();
    add_raster(&mut a);
    a.sync_views();
    begin(&mut a, ctx).unwrap();
    a
}

fn add_raster(a: &mut PhotocraftApp) {
    a.session.execute("layer.new.layer", json!({})).unwrap();
    a.session
        .edit("fixture", |doc, id| {
            doc.layer_mut(id.unwrap()).unwrap().surface_mut().unwrap().fill_rect(Rect::new(0, 0, 160, 140), &[0.2, 0.5, 0.8, 1.0]);
            Ok(())
        })
        .unwrap();
}

fn guides() -> Value {
    json!([{"id":1,"corners":[[25,20],[100,16],[109,105],[21,110]]}])
}

fn view() -> ViewXform {
    ViewXform {
        rect: egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0)),
        zoom: 1.0,
        center: [80.0, 70.0],
        flip: false,
        rotation: 0.0,
        aspect: 1.0,
    }
}

#[test]
fn content_aware_real_entry_points_preserve_global_session_and_normal_open() {
    let ctx = egui::Context::default();
    let mut a = app(&ctx);
    let doc = a.session.active().unwrap().doc.clone();
    let history = a.session.active().unwrap().history.past_len();
    let before = a.distort.global_perspective.as_ref().unwrap().describe();
    let errors = [
        crate::prefs_ui::invoke(&mut a, &ctx, crate::cas_transform::COMMAND, &json!({})).unwrap().unwrap_err(),
        crate::cas_transform::begin(&mut a, &ctx).unwrap_err(),
        crate::transform_tool::begin_content_aware(&mut a, &ctx, Rect::new(0, 0, 160, 140), Default::default()).unwrap_err(),
    ];
    for error in errors {
        assert!(error.contains("Global Perspective Warp"), "{error}");
    }
    assert!(a.ui.transform.is_none() && a.transform_preview.is_none());
    assert_eq!(a.distort.global_perspective.as_ref().unwrap().describe(), before);
    assert!(Arc::ptr_eq(&doc, &a.session.active().unwrap().doc));
    assert_eq!(a.session.active().unwrap().history.past_len(), history);
    control(&mut a, &json!({"cancel":true})).unwrap();
    crate::prefs_ui::invoke(&mut a, &ctx, crate::cas_transform::COMMAND, &json!({})).unwrap().unwrap();
    assert!(a.ui.transform.is_some() && a.transform_preview.is_some());
}

#[test]
fn gallery_double_click_and_direct_open_cannot_strand_global_editor() {
    let ctx = egui::Context::default();
    let mut a = app(&ctx);
    control(&mut a, &json!({"cancel":true})).unwrap();
    a.run("layer.smartObjects.convertToSmartObject", json!({})).unwrap();
    a.run("filter.filterGallery", json!({"effects":[{"filter":"chrome","params":{"detail":2,"smoothness":9}}]})).unwrap();
    let layer = a.session.active().unwrap().active_layer.unwrap();
    let photocraft_doc::LayerContent::Smart(sm) = a.session.active().unwrap().doc.layer(layer).unwrap().content.clone() else {
        panic!("expected smart fixture");
    };
    // A real smart filter on another layer remains double-clickable while the
    // topmost raster is being rectified.
    add_raster(&mut a);
    begin(&mut a, &ctx).unwrap();
    let doc = a.session.active().unwrap().doc.clone();
    let history = a.session.active().unwrap().history.past_len();
    let before = a.distort.global_perspective.as_ref().unwrap().describe();
    assert!(crate::gallery_ui::open(&mut a).unwrap_err().contains("Global Perspective Warp"));
    assert!(crate::gallery_ui::open_smart_filter(&mut a, layer, 0).unwrap_err().contains("Global Perspective Warp"));
    crate::smart_ui::open_editor(&mut a, &ctx, layer.0, &sm, 0);
    assert!(a.ui.status_error && a.ui.status.contains("Global Perspective Warp"));
    assert!(a.distort.gallery.is_none());
    assert_eq!(a.distort.global_perspective.as_ref().unwrap().describe(), before);
    assert!(Arc::ptr_eq(&doc, &a.session.active().unwrap().doc));
    assert_eq!(a.session.active().unwrap().history.past_len(), history);
    control(&mut a, &json!({"cancel":true})).unwrap();
    crate::smart_ui::open_editor(&mut a, &ctx, layer.0, &sm, 0);
    assert!(a.distort.gallery.is_some());
    a.distort.gallery = None;
    crate::gallery_ui::open(&mut a).unwrap();
    assert!(a.distort.gallery.is_some());
}

fn canvas_frame(a: &mut PhotocraftApp, ctx: &egui::Context, events: Vec<egui::Event>, primary: bool) -> egui::FullOutput {
    let rect = view().rect;
    let input = egui::RawInput { screen_rect: Some(rect), events, ..Default::default() };
    let v = crate::state::View { center: [80.0, 70.0], zoom: 1.0, fit_pending: false, ..Default::default() };
    ctx.run_ui(input, |ui| {
        crate::canvas::canvas_view(a, ui, 0, rect, v.clone(), primary);
    })
}

fn button(button: egui::PointerButton, pressed: bool) -> egui::Event {
    egui::Event::PointerButton { pos: pos2(180.0, 150.0), button, pressed, modifiers: egui::Modifiers::NONE }
}

fn navigation(middle: bool, pressed: bool) -> egui::Event {
    if middle {
        button(egui::PointerButton::Middle, pressed)
    } else {
        egui::Event::Key { key: egui::Key::Space, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE }
    }
}

#[test]
fn canvas_navigation_releases_corner_edge_and_new_guide_grabs_for_both_release_orders() {
    for middle in [false, true] {
        for primary_first in [false, true] {
            for start in [[25.0, 20.0], [60.0, 18.0], [120.0, 30.0]] {
                let ctx = egui::Context::default();
                let mut a = app(&ctx);
                control(&mut a, &json!({"guides":guides()})).unwrap();
                canvas_frame(&mut a, &ctx, vec![button(egui::PointerButton::Primary, true)], true).drop_without_applying_deltas();
                pointer(&mut a, ToolEvent::Down { x: start[0], y: start[1], pressure: 1.0 }, egui::Modifiers::NONE);
                pointer(&mut a, ToolEvent::Move { x: start[0] + 10.0, y: start[1] + 3.0, pressure: 1.0 }, egui::Modifiers::NONE);
                assert!(a.distort.global_perspective.as_ref().unwrap().drag.is_some());
                let before = a.distort.global_perspective.as_ref().unwrap().describe()["guides"].clone();
                canvas_frame(&mut a, &ctx, vec![navigation(middle, true)], true).drop_without_applying_deltas();
                assert!(a.distort.global_perspective.as_ref().unwrap().drag.is_none());
                let primary_up = button(egui::PointerButton::Primary, false);
                let nav_up = navigation(middle, false);
                let releases = if primary_first { [primary_up, nav_up] } else { [nav_up, primary_up] };
                for release in releases {
                    canvas_frame(&mut a, &ctx, vec![release], true).drop_without_applying_deltas();
                }
                pointer(&mut a, ToolEvent::Move { x: 50.0, y: 50.0, pressure: 1.0 }, egui::Modifiers::NONE);
                assert_eq!(a.distort.global_perspective.as_ref().unwrap().describe()["guides"], before);
            }
        }
    }
}

#[test]
fn secondary_view_does_not_release_primary_guide_drag() {
    let ctx = egui::Context::default();
    let mut a = app(&ctx);
    control(&mut a, &json!({"guides":guides()})).unwrap();
    pointer(&mut a, ToolEvent::Down { x: 25.0, y: 20.0, pressure: 1.0 }, egui::Modifiers::NONE);
    canvas_frame(&mut a, &ctx, vec![navigation(true, true)], false).drop_without_applying_deltas();
    assert!(a.distort.global_perspective.as_ref().unwrap().drag.is_some());
}

#[test]
fn global_guide_pointer_owner_retains_ctrl_move_but_yields_view_navigation() {
    let owns = crate::canvas::global_guides_own_primary_input;
    // Ctrl temporary Move is for the guide's axis lock, not a view gesture.
    assert!(owns(true, true, false, false, false));
    // Hand/Zoom navigation, dialogs, middle-button and secondary views yield.
    assert!(!owns(true, true, false, true, false));
    assert!(!owns(true, true, true, false, false));
    assert!(!owns(true, true, false, false, true));
    assert!(!owns(true, false, false, false, false));
}

#[test]
fn ctrl_temporary_move_does_not_cancel_the_global_axis_drag() {
    let ctx = egui::Context::default();
    let mut a = app(&ctx);
    a.ui.tool = crate::state::Tool::Brush;
    control(&mut a, &json!({"guides":guides()})).unwrap();
    pointer(&mut a, ToolEvent::Down { x: 25.0, y: 20.0, pressure: 1.0 }, egui::Modifiers::NONE);
    let ctrl = egui::Modifiers { ctrl: true, command: true, ..Default::default() };
    let input = egui::RawInput { screen_rect: Some(view().rect), events: vec![egui::Event::ModifiersChanged(ctrl)], ..Default::default() };
    let output = ctx.run_ui(input, |ui| {
        let v = crate::state::View { center: [80.0, 70.0], fit_pending: false, ..Default::default() };
        crate::canvas::canvas_view(&mut a, ui, 0, view().rect, v, true);
    });
    output.drop_without_applying_deltas();
    assert!(a.distort.global_perspective.as_ref().unwrap().drag.is_some());
    pointer(&mut a, ToolEvent::Move { x: 40.0, y: 22.0, pressure: 1.0 }, ctrl);
    assert_eq!(a.distort.global_perspective.as_ref().unwrap().guides[0].corners[0], [40.0, 20.0]);
}

#[test]
fn mesh_repaints_share_storage_and_all_view_components_invalidate_it() {
    let ctx = egui::Context::default();
    let mut a = app(&ctx);
    control(&mut a, &json!({"guides":guides(),"solve":true})).unwrap();
    let s = a.distort.global_perspective.as_mut().unwrap();
    // Exercise the maximum accepted budget as well as the ordinary fast path.
    for n in [GRID, MAX_PREVIEW_GRID] {
        s.grid_size = n;
        s.invalidate_mesh();
        let mut xf = view();
        let mut mesh = s.preview_mesh(&xf);
        assert_eq!(mesh.vertices.len(), (n + 1) * (n + 1));
        assert_eq!(mesh.indices.len(), n * n * 6);
        assert!(Arc::ptr_eq(&mesh, &s.preview_mesh(&xf)));
        let output = ctx.run_ui(Default::default(), |ui| draw(s, ui.painter(), &xf));
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Mesh(drawn) if Arc::ptr_eq(&mesh, drawn))));
        output.drop_without_applying_deltas();
        for change in 0..6 {
            match change {
                0 => xf.zoom = 2.0,
                1 => xf.center[0] += 12.0,
                2 => xf.rect = xf.rect.translate(egui::vec2(20.0, 10.0)),
                3 => xf.rotation = 30.0,
                4 => xf.flip = true,
                _ => xf.aspect = 1.5,
            }
            let next = s.preview_mesh(&xf);
            assert!(!Arc::ptr_eq(&mesh, &next));
            let p = s.map([s.bounds.x0 as f64, s.bounds.y0 as f64]);
            assert_eq!(next.vertices[0].pos, xf.to_screen(p[0] as f32, p[1] as f32));
            assert!(Arc::ptr_eq(&next, &s.preview_mesh(&xf)));
            mesh = next;
        }
    }
}

#[test]
fn mesh_selection_keeps_cache_but_solution_layout_and_guide_changes_invalidate() {
    let ctx = egui::Context::default();
    let mut a = app(&ctx);
    control(&mut a, &json!({"guides":guides(),"solve":true})).unwrap();
    let xf = view();
    let first = a.distort.global_perspective.as_mut().unwrap().preview_mesh(&xf);
    control(&mut a, &json!({"selectedGuide":1})).unwrap();
    assert!(Arc::ptr_eq(&first, &a.distort.global_perspective.as_mut().unwrap().preview_mesh(&xf)));
    let mut previous = first;
    for p in [json!({"mode":"layout"}), json!({"options":{"aspect":1.2},"solve":true}), json!({"guides":guides()})] {
        control(&mut a, &p).unwrap();
        let s = a.distort.global_perspective.as_mut().unwrap();
        assert!(s.mesh.is_none());
        let next = s.preview_mesh(&xf);
        assert!(!Arc::ptr_eq(&previous, &next));
        let mapped = s.map([s.bounds.x1 as f64, s.bounds.y1 as f64]);
        assert_eq!(next.vertices.last().unwrap().pos, xf.to_screen(mapped[0] as f32, mapped[1] as f32));
        previous = next;
    }
}

#[test]
fn secondary_canvas_paints_complete_source_and_never_overwrites_primary_preview() {
    let ctx = egui::Context::default();
    let mut a = app(&ctx);
    let source = a.session.active().unwrap().doc.clone();
    control(&mut a, &json!({"guides":guides(),"solve":true})).unwrap();
    let expected = crate::canvas::display_image(None, &photocraft_compose::render_reduced(&source, 160, 140));
    let primary_texture = crate::canvas::ensure_texture(&mut a, &ctx, 0, None).unwrap().0;
    let mut output = canvas_frame(&mut a, &ctx, Vec::new(), false);
    let texture = a.distort.global_perspective.as_ref().unwrap().source_textures.last().unwrap().2.id();
    assert_ne!(texture, primary_texture);
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Mesh(mesh) if mesh.texture_id == texture)));
    let egui::ImageData::Color(image) = &output.textures_delta.set.get(&texture).unwrap()[0].image;
    assert_eq!(image.pixels, expected.pixels, "secondary image includes the unwarped target layer");
    output.textures_delta.clear();
    assert_eq!(crate::canvas::global_source_texture(&mut a, &ctx, 0, None, false), Some(texture));
    assert!(crate::canvas::global_source_texture(&mut a, &ctx, 0, None, true).is_none());
    let primary = canvas_frame(&mut a, &ctx, Vec::new(), true);
    let s = a.distort.global_perspective.as_ref().unwrap();
    assert!(primary.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Mesh(mesh) if mesh.texture_id == s.texture.id())));
    primary.drop_without_applying_deltas();
    assert_eq!(crate::canvas::ensure_texture(&mut a, &ctx, 0, None).unwrap().0, primary_texture);
    assert!(Arc::ptr_eq(&source, &a.session.active().unwrap().doc));
    control(&mut a, &json!({"cancel":true})).unwrap();
    assert!(ctx.tex_manager().read().meta(texture).is_none(), "the source cache is released with the session");
}

#[test]
fn secondary_source_cache_tracks_monitor_texture_limit_and_source_lifetime() {
    use photocraft_engine::display_color::Display;
    let ctx = egui::Context::default();
    let mut a = app(&ctx);
    control(&mut a, &json!({"cancel":true})).unwrap();
    let icc = |id| Some(photocraft_engine::color_cmds::resolve_profile(id, None, None).unwrap().to_bytes());
    a.session.color.set_displays(Ok(vec![
        Display { id: 1, name: "A".into(), frame: [0.0, 0.0, 100.0, 100.0], profile_name: None, icc: icc("srgb") },
        Display { id: 4, name: "B".into(), frame: [100.0, 0.0, 100.0, 100.0], profile_name: None, icc: icc("display-p3") },
    ]));
    a.session.color.main_display = Some(1);
    begin(&mut a, &ctx).unwrap();
    let first = crate::canvas::global_source_texture(&mut a, &ctx, 0, Some(1), false).unwrap();
    let p3 = crate::canvas::global_source_texture(&mut a, &ctx, 0, Some(4), false).unwrap();
    assert_ne!(first, p3);
    // Two secondary windows on different monitors must never evict each other
    // each frame, or the full document would be recomposited continuously.
    assert_eq!(crate::canvas::global_source_texture(&mut a, &ctx, 0, Some(1), false), Some(first));
    assert_eq!(crate::canvas::global_source_texture(&mut a, &ctx, 0, Some(4), false), Some(p3));
    assert_eq!(a.distort.global_perspective.as_ref().unwrap().source_textures.len(), 2);
    let source = a.session.active().unwrap().doc.clone();
    let (display, _) = crate::canvas::canvas_display(&a, &source, Some(4));
    assert!(!display.as_ref().unwrap().is_identity());
    let expected = crate::canvas::display_image(display.as_deref(), &photocraft_compose::render_reduced(&source, 160, 140));
    let mut delta = ctx.tex_manager().write().take_delta();
    let egui::ImageData::Color(image) = &delta.set.get(&p3).unwrap()[0].image;
    assert_eq!(image.pixels, expected.pixels);
    delta.clear();
    assert_eq!(crate::canvas::global_source_texture(&mut a, &ctx, 0, Some(4), false), Some(p3));
    ctx.input_mut(|i| i.max_texture_side = 64);
    let reduced = crate::canvas::global_source_texture(&mut a, &ctx, 0, Some(4), false).unwrap();
    assert_ne!(p3, reduced);
    assert_eq!(a.distort.global_perspective.as_ref().unwrap().source_textures.len(), 1);
    assert!(ctx.tex_manager().read().meta(reduced).unwrap().size.into_iter().all(|side| side <= 64));
    a.session.execute("layer.new.layer", json!({})).unwrap();
    assert!(crate::canvas::global_source_texture(&mut a, &ctx, 0, Some(4), false).is_none());
    assert!(crate::distort_ui::display_doc(&a, 0).is_none());
}

#[test]
fn place_entry_points_reject_before_mutating_global_warp_source() {
    let ctx = egui::Context::default();
    let mut a = app(&ctx);
    let doc = a.session.active().unwrap().doc.clone();
    let history = a.session.active().unwrap().history.past_len();
    let before = a.distort.global_perspective.as_ref().unwrap().describe();
    for cmd in ["file.placeEmbedded", "file.placeLinked"] {
        let error = a.run(cmd, json!({"path":"__nonexistent_global_place_probe__.png"})).unwrap_err();
        assert!(error.contains("Global Perspective Warp"), "{cmd}: {error}");
        let error = crate::view_cmds::invoke(&mut a, &ctx, cmd, &json!({})).unwrap().unwrap_err();
        assert!(error.contains("Global Perspective Warp"), "{cmd} picker: {error}");
    }
    let error = a.place_bytes("probe.png", vec![0, 1, 2, 3], None).unwrap_err();
    assert!(error.contains("Global Perspective Warp"), "in-memory place: {error}");
    a.drop_places.push_back(crate::file_open::DropPlace::Bytes("dropped.png".into(), vec![0, 1, 2, 3]));
    a.place_next_dropped(&ctx);
    assert!(a.drop_places.is_empty(), "a rejected drop must not be replayed after warp closes");
    assert!(a.ui.status.contains("Global Perspective Warp"), "drop status: {}", a.ui.status);
    assert_eq!(a.distort.global_perspective.as_ref().unwrap().describe(), before);
    assert!(Arc::ptr_eq(&doc, &a.session.active().unwrap().doc));
    assert_eq!(a.session.active().unwrap().history.past_len(), history);

    control(&mut a, &json!({"cancel":true})).unwrap();
    assert!(!a.place_bytes("probe.png", vec![0, 1, 2, 3], None).unwrap_err().contains("Global Perspective Warp"));
}

#[test]
fn right_button_erase_and_alt_resize_do_not_touch_active_global_guides() {
    for alt_resize in [false, true] {
        let ctx = egui::Context::default();
        let mut a = app(&ctx);
        a.ui.tool = crate::state::Tool::Brush;
        if !alt_resize {
            a.run("prefs.set", json!({"path":"tools.rightClickWithPaintingTools","value":"erase"})).unwrap();
        }
        control(&mut a, &json!({"guides":guides()})).unwrap();
        let before = a.distort.global_perspective.as_ref().unwrap().describe();
        let doc = a.session.active().unwrap().doc.clone();
        let history = a.session.active().unwrap().history.past_len();
        let start = view().to_screen(25.0, 20.0);
        let end = view().to_screen(70.0, 65.0);
        let mods = egui::Modifiers { alt: alt_resize, ..Default::default() };
        let right_event = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Secondary, pressed, modifiers: mods };
        canvas_frame(&mut a, &ctx, vec![egui::Event::PointerMoved(start), right_event(start, true)], true).drop_without_applying_deltas();
        canvas_frame(&mut a, &ctx, vec![egui::Event::PointerMoved(end)], true).drop_without_applying_deltas();
        canvas_frame(&mut a, &ctx, vec![right_event(end, false)], true).drop_without_applying_deltas();

        // Agents can invoke the same right-button gestures through ui.pointer.
        let (request, _rx) = crate::control::ControlRequest::new(
            "ui.pointer",
            json!({
                "tool":"brush", "button":"right", "alt":alt_resize,
                "events":[{"kind":"down","x":25,"y":20},{"kind":"move","x":70,"y":65},{"kind":"up","x":70,"y":65}]
            }),
        );
        let outcome = crate::control::handle(&mut a, &ctx, &request);
        let crate::control::Outcome::Done(reply) = outcome else {
            panic!("right-button ui.pointer did not finish");
        };
        assert_eq!(reply["ok"], true, "right-button ui.pointer: {reply}");
        let s = a.distort.global_perspective.as_ref().unwrap();
        assert_eq!(s.describe(), before, "secondary drag changed guide state (alt={alt_resize})");
        assert!(s.drag.is_none());
        assert!(Arc::ptr_eq(&doc, &a.session.active().unwrap().doc));
        assert_eq!(a.session.active().unwrap().history.past_len(), history);
        assert!(!a.secondary_erase && !a.brush_resize_armed);
    }
}

#[test]
fn primary_guide_drag_release_is_not_lost_while_secondary_is_held() {
    let ctx = egui::Context::default();
    let mut a = app(&ctx);
    control(&mut a, &json!({"guides":guides()})).unwrap();
    let start = pos2(180.0, 150.0);
    let end = pos2(230.0, 190.0);
    let button_at = |pos, button, pressed| egui::Event::PointerButton { pos, button, pressed, modifiers: egui::Modifiers::NONE };
    canvas_frame(&mut a, &ctx, vec![egui::Event::PointerMoved(start), button_at(start, egui::PointerButton::Primary, true)], true)
        .drop_without_applying_deltas();
    // Start the known corner grab while egui owns the primary pointer.
    pointer(&mut a, ToolEvent::Down { x: 25.0, y: 20.0, pressure: 1.0 }, egui::Modifiers::NONE);
    canvas_frame(&mut a, &ctx, vec![egui::Event::PointerMoved(end)], true).drop_without_applying_deltas();
    assert!(a.distort.global_perspective.as_ref().unwrap().drag.is_some());
    canvas_frame(&mut a, &ctx, vec![button_at(end, egui::PointerButton::Secondary, true)], true).drop_without_applying_deltas();
    canvas_frame(&mut a, &ctx, vec![button_at(end, egui::PointerButton::Primary, false)], true).drop_without_applying_deltas();
    assert!(a.distort.global_perspective.as_ref().unwrap().drag.is_none(), "primary release during secondary hold must terminate the guide grab");
    canvas_frame(&mut a, &ctx, vec![button_at(end, egui::PointerButton::Secondary, false)], true).drop_without_applying_deltas();
    assert!(a.distort.global_perspective.as_ref().unwrap().drag.is_none());
}
