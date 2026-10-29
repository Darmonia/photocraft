//! Focused P2 regressions for the V3.2 guide editor, separate from legacy warp.
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

fn guide() -> Value {
    json!([{"id":1,"corners":[[25,20],[100,16],[109,105],[21,110]]}])
}

#[test]
fn unsupported_layer_compositing_is_rejected_without_mutating_pixels_or_history() {
    for case in 0..6 {
        let mut a = app();
        a.distort.global_perspective = None;
        a.session
            .edit("unsupported preview metadata", |doc, active| {
                let layer = doc.layer_mut(active.unwrap()).unwrap();
                match case {
                    0 => layer.blend = photocraft_doc::BlendMode::Multiply,
                    1 => layer.clipped = true,
                    2 => layer.blend_if.set(0, [photocraft_doc::BlendRange { black: [40, 40], white: [255, 255] }, photocraft_doc::BlendRange::FULL]),
                    3 => layer.advanced.knockout = photocraft_doc::Knockout::Deep,
                    4 => layer.excluded_channels = 1,
                    5 => layer.visible = false,
                    _ => unreachable!(),
                }
                Ok(())
            })
            .unwrap();
        let doc = a.session.active().unwrap().doc.clone();
        let history = a.session.active().unwrap().history.past_len();
        let error = begin(&mut a, &egui::Context::default()).unwrap_err();
        assert!(error.contains("preview"), "case {case}: {error}");
        assert!(a.distort.global_perspective.is_none());
        assert!(Arc::ptr_eq(&doc, &a.session.active().unwrap().doc));
        assert_eq!(history, a.session.active().unwrap().history.past_len());
    }
}

#[test]
fn foreground_and_nested_layers_cannot_be_painted_over_by_a_raw_preview_mesh() {
    for nested in [false, true] {
        let mut a = app();
        a.distort.global_perspective = None;
        a.session
            .edit("foreground or nested fixture", |doc, active| {
                if nested {
                    let index = doc.layers.iter().position(|l| l.id == active.unwrap()).unwrap();
                    let target = doc.layers.remove(index);
                    doc.layers.push(photocraft_doc::Layer::group("container", vec![target]));
                } else {
                    let format = doc.pixel_format();
                    doc.layers.push(photocraft_doc::Layer::raster("foreground", format));
                }
                Ok(())
            })
            .unwrap();
        let error = begin(&mut a, &egui::Context::default()).unwrap_err();
        assert!(error.contains("topmost"), "{error}");
        assert!(a.distort.global_perspective.is_none());
    }
}

#[test]
fn disabled_guide_at_projective_horizon_stays_in_safe_source_space() {
    let mut a = app();
    let mut guides = guide().as_array().unwrap().clone();
    guides.push(json!({
        "id":99, "enabled":false,
        "corners":[[-60,10],[-45,10],[-45,30],[-60,30]]
    }));
    control(&mut a, &json!({"guides":guides,"solve":true})).unwrap();
    let s = a.distort.global_perspective.as_mut().unwrap();
    let report = s.solution.as_ref().unwrap().guides.iter().find(|r| r.id == 99).unwrap();
    assert!(!report.used && report.transformed_corners.is_none());
    // The image-domain homography may have an off-image pole at x=-60.
    // Rendering or hit-testing this unused guide must not evaluate that pole.
    s.solution.as_mut().unwrap().matrix = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0 / 60.0, 0.0, 1.0];
    let unused = s.guides.iter().find(|g| g.id == 99).unwrap();
    assert_eq!(s.display_corners(unused), Some(unused.corners));
    assert!(matches!(hit_handle(s, [-60.0, 10.0], 1.0), Some(Handle::Corner { id: 99, .. })));
    let used = s.guides.iter().find(|g| g.id == 1).unwrap();
    let validated = s.solution.as_ref().unwrap().guides.iter().find(|g| g.id == 1).unwrap().transformed_corners;
    assert_eq!(s.display_corners(used), validated);
    s.show_result = false;
    assert_eq!(s.display_corners(used), Some(used.corners));
}

#[test]
fn ctrl_dead_zone_is_two_screen_points_at_multiple_zooms_for_corners_and_edges() {
    let ctrl = egui::Modifiers { ctrl: true, shift: true, ..Default::default() };
    for zoom in [0.1_f32, 1.0, 16.0] {
        let mut a = app();
        let active = a.session.active_index().unwrap();
        a.ui.views[active].zoom = zoom;
        control(&mut a, &json!({"guides":guide()})).unwrap();
        let z = f64::from(zoom);
        let threshold = crate::distort_ui::tolerance(&a) * 0.25;
        assert!((threshold * z - 2.0).abs() < 1e-5);

        pointer(&mut a, ToolEvent::Down { x: 25.0, y: 20.0, pressure: 1.0 }, egui::Modifiers::NONE);
        let s = a.distort.global_perspective.as_ref().unwrap();
        assert!(matches!(s.drag.as_ref(), Some(Drag::Corner { activation_distance, .. }) if (*activation_distance - threshold).abs() < 1e-5));
        pointer(&mut a, ToolEvent::Move { x: 25.0 + 1.0 / z, y: 20.0 + 0.1 / z, pressure: 1.0 }, ctrl);
        assert_eq!(a.distort.global_perspective.as_ref().unwrap().guides[0].corners[0], [25.0, 20.0]);
        pointer(&mut a, ToolEvent::Move { x: 25.0 + 3.0 / z, y: 20.0 + 0.2 / z, pressure: 1.0 }, ctrl);
        let c = a.distort.global_perspective.as_ref().unwrap().guides[0].corners[0];
        assert!(c[0] > 25.0 && c[1] == 20.0, "Ctrl must win over Shift at zoom {zoom}");

        // Explicit edge drag tests the same screen-space threshold, independent of
        // the corner hit-area at very low zoom.
        let s = a.distort.global_perspective.as_mut().unwrap();
        s.drag = Some(Drag::Edge { id: 1, edge: 0, mouse_start: [60.0, 18.0], original: s.guides[0].corners, axis: None, activation_distance: threshold });
        let old = s.guides[0].corners;
        s.drag_to([60.0 + 0.2 / z, 18.0 + 1.0 / z], ctrl);
        assert_eq!(s.guides[0].corners, old);
        s.drag_to([60.0 + 0.2 / z, 18.0 + 3.0 / z], ctrl);
        assert_eq!(s.guides[0].corners[0][0], old[0][0]);
        assert!(s.guides[0].corners[0][1] > old[0][1]);
    }
}

#[test]
fn stale_source_keeps_visible_options_bar_and_non_destructive_cancel() {
    let mut a = app();
    control(&mut a, &json!({"guides":guide(),"solve":true})).unwrap();
    a.session.execute("layer.new.layer", json!({})).unwrap();
    let current = a.session.active().unwrap().doc.clone();
    let history = a.session.active().unwrap().history.past_len();
    let idx = a.session.active_index().unwrap();
    assert!(crate::distort_ui::display_doc(&a, idx).is_none());
    let ctx = egui::Context::default();
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        assert!(crate::distort_ui::options_bar(&mut a, ui), "stale session must retain visible Cancel");
    });
    output.drop_without_applying_deltas();
    assert!(commit(&mut a).is_err(), "a stale session cannot commit");
    assert_eq!(control(&mut a, &json!({"cancel":true})).unwrap()["cancelled"], true);
    assert!(a.distort.global_perspective.is_none());
    assert!(Arc::ptr_eq(&current, &a.session.active().unwrap().doc));
    assert_eq!(history, a.session.active().unwrap().history.past_len());
}

#[test]
fn raw_warp_preview_rejects_color_managed_and_hdr_documents() {
    let mut a = app();
    a.distort.global_perspective = None;
    a.session
        .edit("embedded Adobe RGB", |doc, _| {
            doc.icc_profile = Some(photocraft_cms::Builtin::AdobeRgbCompat.profile().to_bytes());
            Ok(())
        })
        .unwrap();
    let original = a.session.active().unwrap().doc.clone();
    let history = a.session.active().unwrap().history.past_len();
    let error = begin(&mut a, &egui::Context::default()).unwrap_err();
    assert!(error.contains("color transform"), "{error}");
    assert!(Arc::ptr_eq(&original, &a.session.active().unwrap().doc));
    assert_eq!(history, a.session.active().unwrap().history.past_len());

    let mut hdr = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    hdr.session.execute("file.new", json!({"width":32,"height":24,"depth":32})).unwrap();
    hdr.session.execute("layer.new.layer", json!({})).unwrap();
    hdr.session
        .edit("HDR fixture", |doc, id| {
            doc.layer_mut(id.unwrap()).unwrap().surface_mut().unwrap().fill_rect(Rect::new(0, 0, 32, 24), &[1.5, 0.5, 0.2, 1.0]);
            Ok(())
        })
        .unwrap();
    hdr.sync_views();
    let error = begin(&mut hdr, &egui::Context::default()).unwrap_err();
    assert!(error.contains("8-bit RGB"), "{error}");
    assert!(hdr.distort.global_perspective.is_none());
}

#[test]
fn changing_monitor_color_while_editing_invalidates_preview_but_keeps_cancel() {
    let mut a = app();
    let idx = a.session.active_index().unwrap();
    assert!(bound_to_active_document(&a, a.distort.global_perspective.as_ref().unwrap()));
    a.session.color.settings.monitor_profile = "adobe-rgb-compat".into();
    assert!(!bound_to_active_document(&a, a.distort.global_perspective.as_ref().unwrap()));
    assert!(crate::distort_ui::display_doc(&a, idx).is_none());
    assert!(commit(&mut a).is_err());
    assert_eq!(control(&mut a, &json!({"cancel":true})).unwrap()["cancelled"], true);
}

#[test]
fn competing_free_transform_and_distortion_sessions_cannot_replace_global_editor() {
    let mut a = app();
    let original = a.session.active().unwrap().doc.clone();
    let history = a.session.active().unwrap().history.past_len();
    let key = a.distort.global_perspective.as_ref().unwrap().key;
    let ctx = egui::Context::default();
    for id in [
        "edit.puppetWarp",
        "layer.smartObjects.puppetWarp",
        "edit.perspectiveWarp",
        "layer.smartObjects.perspectiveWarp",
        "filter.vanishingPoint",
        "filter.liquify",
        "filter.filterGallery",
    ] {
        let error = crate::distort_ui::menu(&mut a, &ctx, id, &json!({})).unwrap().unwrap_err();
        assert!(error.contains("Global Perspective Warp"), "{id}: {error}");
    }
    for action in [
        crate::transform_tool::begin,
        crate::transform_tool::begin_copy,
        crate::transform_tool::begin_placed,
        crate::transform_tool::begin_selection,
        crate::transform_tool::begin_warp,
    ] {
        let error = action(&mut a, &ctx).unwrap_err();
        assert!(error.contains("Global Perspective Warp"), "{error}");
    }
    assert!(a.ui.transform.is_none());
    assert!(a.distort.gallery.is_none() && a.distort.liquify.is_none() && a.distort.puppet.is_none());
    assert!(a.distort.perspective.is_none() && a.distort.vanishing.is_none());
    assert_eq!(a.distort.global_perspective.as_ref().unwrap().key, key);
    assert!(Arc::ptr_eq(&original, &a.session.active().unwrap().doc));
    assert_eq!(history, a.session.active().unwrap().history.past_len());
}

#[test]
fn active_guide_editor_takes_view_tool_primary_drags_but_yields_to_temporary_navigation() {
    for tool in [crate::state::Tool::Hand, crate::state::Tool::Zoom, crate::state::Tool::RotateView] {
        assert!(crate::canvas::global_guides_own_primary_input(true, true, false, false, false));
        assert!(!crate::canvas::global_guides_own_primary_input(true, true, false, true, false), "Space or temporary Zoom must navigate");
        assert!(!crate::canvas::global_guides_own_primary_input(true, true, false, false, true), "Middle button must pan");
        assert!(!crate::canvas::global_guides_own_primary_input(false, true, false, false, false));
        assert!(!crate::canvas::global_guides_own_primary_input(true, false, false, false, false));
        assert!(!crate::canvas::global_guides_own_primary_input(true, true, true, false, false));
        let mut a = app();
        a.ui.tool = tool;
        crate::canvas::tool_event(&mut a, ToolEvent::Down { x: 10.0, y: 10.0, pressure: 1.0 }, egui::Modifiers::NONE);
        crate::canvas::tool_event(&mut a, ToolEvent::Up { x: 65.0, y: 75.0 }, egui::Modifiers::NONE);
        assert_eq!(a.distort.global_perspective.as_ref().unwrap().guides.len(), 1);
    }
}

#[test]
fn preview_mesh_adapts_to_projective_error_or_refuses_misleading_result() {
    let bounds = Rect::new(0, 0, 4096, 64);
    let affine = photocraft_algo::transform::Homography([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
    assert_eq!(preview_grid_size(bounds, &affine).unwrap(), GRID);
    let perspective = photocraft_algo::transform::Homography([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.01, 0.0, 1.0]);
    let refined = preview_grid_size(bounds, &perspective).unwrap();
    assert!(refined > GRID, "stronger perspective should refine beyond the default {GRID}");
    assert!(preview_mesh_exceeds_error(bounds, &perspective, GRID));
    assert!(!preview_mesh_exceeds_error(bounds, &perspective, refined));
    let too_steep = photocraft_algo::transform::Homography([1000.0, 0.0, 0.0, 0.0, 1000.0, 0.0, 0.01, 0.0, 1.0]);
    assert!(preview_grid_size(bounds, &too_steep).is_err(), "unrepresentable mesh cannot be displayed as accurate");
}
