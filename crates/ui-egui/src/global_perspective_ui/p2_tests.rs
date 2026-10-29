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
                    2 => layer.blend_if.set(
                        0,
                        [
                            photocraft_doc::BlendRange { black: [40, 40], white: [255, 255] },
                            photocraft_doc::BlendRange::FULL,
                        ],
                    ),
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
        s.drag = Some(Drag::Edge {
            id: 1,
            edge: 0,
            mouse_start: [60.0, 18.0],
            original: s.guides[0].corners,
            axis: None,
            activation_distance: threshold,
        });
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
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            assert!(crate::distort_ui::options_bar(&mut a, ui), "stale session must retain visible Cancel");
        });
    });
    assert!(commit(&mut a).is_err(), "a stale session cannot commit");
    assert_eq!(control(&mut a, &json!({"cancel":true})).unwrap()["cancelled"], true);
    assert!(a.distort.global_perspective.is_none());
    assert!(Arc::ptr_eq(&current, &a.session.active().unwrap().doc));
    assert_eq!(history, a.session.active().unwrap().history.past_len());
}
