use super::*;
use escope_host::protocol::{capability, CaptureMeta, PixelFormat};

fn fixture() -> (
    Inspector,
    mpsc::Sender<Update>,
    tokio::sync::mpsc::Receiver<Command>,
) {
    let (commands, receiver) = tokio::sync::mpsc::channel(16);
    let (sender, updates) = mpsc::channel();
    let app = Inspector::with_channels("tcp://127.0.0.1:9337".into(), commands, updates);
    (app, sender, receiver)
}

fn node(revision: u32) -> UiNode {
    UiNode {
        id: 2,
        parent: 0,
        kind: "button".into(),
        name: "设置 / Increment".into(),
        x: 40,
        y: 80,
        width: 100,
        height: 60,
        flags: 0,
        revision,
    }
}

fn capture() -> Capture {
    Capture::from_raw(
        CaptureMeta {
            capture_id: 1,
            width: 320,
            height: 240,
            stride: 960,
            format: PixelFormat::Rgb888,
            total_bytes: 320 * 240 * 3,
            frame_id: 7,
        },
        &vec![80; 320 * 240 * 3],
    )
    .unwrap()
}

fn connect(sender: &mpsc::Sender<Update>, caps: u32) {
    sender
        .send(Update::Connected(
            DeviceInfo {
                name: "UI test device".into(),
                model: "software / synthetic".into(),
                firmware: "test".into(),
                build_id: "headless".into(),
                boot_id: 1,
                uptime_ms: 0,
            },
            Hello {
                session: 1,
                capabilities: caps,
                max_payload: 1024,
                protocol: 1,
                name: "UI test device".into(),
                build_id: "headless".into(),
            },
        ))
        .unwrap();
}

fn frame(
    app: &mut Inspector,
    ctx: &egui::Context,
    size: egui::Vec2,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ctx| app.render(ctx),
    )
}

fn image_rect(output: &egui::FullOutput, texture: egui::TextureId) -> egui::Rect {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Mesh(mesh) if mesh.texture_id == texture => Some(mesh.calc_bounds()),
            egui::Shape::Rect(rect)
                if rect
                    .brush
                    .as_ref()
                    .is_some_and(|brush| brush.fill_texture_id == texture) =>
            {
                Some(rect.rect)
            }
            _ => None,
        })
        .expect("captured display mesh")
}

fn text_rects(output: &egui::FullOutput) -> Vec<(String, egui::Rect, egui::Rect)> {
    fn collect(
        shape: &egui::Shape,
        clip: egui::Rect,
        out: &mut Vec<(String, egui::Rect, egui::Rect)>,
    ) {
        match shape {
            egui::Shape::Text(text) => out.push((
                text.galley.job.text.clone(),
                text.visual_bounding_rect(),
                clip,
            )),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, clip, out);
                }
            }
            _ => (),
        }
    }
    let mut out = Vec::new();
    for shape in &output.shapes {
        collect(&shape.shape, shape.clip_rect, &mut out);
    }
    out
}

#[test]
fn selection_and_input_use_the_centered_scaled_image_at_both_window_sizes() {
    for size in [egui::vec2(900.0, 620.0), egui::vec2(1380.0, 900.0)] {
        let (mut app, sender, mut commands) = fixture();
        let ctx = egui::Context::default();
        theme::apply(&ctx, true);
        connect(
            &sender,
            capability::UI_TREE | capability::CAPTURE | capability::INPUT,
        );
        sender
            .send(Update::Snapshot(vec![node(1)], None, Some(capture())))
            .unwrap();
        frame(&mut app, &ctx, size, vec![]);
        let output = frame(&mut app, &ctx, size, vec![]);
        let rect = image_rect(&output, app.texture.as_ref().unwrap().id());
        assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(rect));
        let point = rect.min + rect.size() * egui::vec2(80.5 / 320.0, 100.5 / 240.0);
        let click = |app: &mut Inspector| {
            frame(
                app,
                &ctx,
                size,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                ],
            );
            frame(
                app,
                &ctx,
                size,
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
            );
        };
        click(&mut app);
        assert_eq!(app.selected, Some(2));
        assert!(
            commands.try_recv().is_err(),
            "inspection must not send device input"
        );
        app.inject = true;
        click(&mut app);
        assert!(matches!(commands.try_recv(), Ok(Command::Tap(80, 100))));
        for label in ["Disconnect", "Refresh", "Live · 1s", "Save PNG"] {
            let (_, bounds, clip) = text_rects(&output)
                .into_iter()
                .find(|(text, _, _)| text == label)
                .unwrap_or_else(|| panic!("{label} missing at {size:?}"));
            assert!(
                clip.expand(1.0).contains_rect(bounds),
                "{label} clipped at {size:?}: {bounds:?} / {clip:?}"
            );
        }
    }
}

#[test]
fn selection_lifecycle_and_updates_continue_in_every_view() {
    let (mut app, sender, _) = fixture();
    let ctx = egui::Context::default();
    connect(&sender, capability::UI_TREE);
    sender
        .send(Update::Snapshot(vec![node(7)], None, None))
        .unwrap();
    app.receive(&ctx);
    app.selected = Some(2);
    app.error = Some("earlier snapshot failed".into());
    sender.send(Update::LogGap(11)).unwrap();
    sender
        .send(Update::Snapshot(vec![node(7)], None, None))
        .unwrap();
    app.receive(&ctx);
    assert_eq!(app.selected, Some(2));
    assert!(app.error.is_none());
    assert_eq!(
        app.host_skipped_logs, 11,
        "snapshot success retains host log loss"
    );
    sender
        .send(Update::Snapshot(vec![node(8)], None, None))
        .unwrap();
    app.receive(&ctx);
    assert_eq!(app.selected, None);
    for (index, view) in [View::Inspector, View::Logs, View::Metrics, View::Device]
        .into_iter()
        .enumerate()
    {
        app.view = view;
        sender
            .send(Update::Log(LogRecord {
                session: 1,
                seq: index as u64,
                timestamp_ms: 0,
                level: 3,
                target: "test".into(),
                message: "event".into(),
                dropped: 0,
                truncated: false,
            }))
            .unwrap();
        frame(&mut app, &ctx, egui::vec2(900.0, 620.0), vec![]);
        assert_eq!(app.logs.len(), index + 1);
    }
    app.auto = true;
    app.inject = true;
    app.selected = Some(2);
    sender.send(Update::Disconnected).unwrap();
    app.receive(&ctx);
    assert!(
        app.info.is_none() && app.hello.is_none() && app.nodes.is_empty() && app.texture.is_none()
    );
    assert!(!app.auto && !app.inject && app.selected.is_none());
    assert_eq!(app.host_skipped_logs, 11);
    assert_eq!(
        app.logs.len(),
        4,
        "disconnected logs remain available as retained events"
    );
}

#[test]
fn capability_and_filtered_log_empty_states_keep_truthful_messages() {
    let (mut app, sender, _) = fixture();
    let ctx = egui::Context::default();
    theme::apply(&ctx, true);
    connect(&sender, capability::LOGS);
    sender
        .send(Update::Log(LogRecord {
            session: 1,
            seq: 1,
            timestamp_ms: 2,
            level: 2,
            target: "ui.layout".into(),
            message: "设置 timeout".into(),
            dropped: 9,
            truncated: true,
        }))
        .unwrap();
    for _ in 0..2 {
        frame(&mut app, &ctx, egui::vec2(900.0, 620.0), vec![]);
    }
    let output = frame(&mut app, &ctx, egui::vec2(900.0, 620.0), vec![]);
    assert!(text_rects(&output)
        .iter()
        .any(|(text, _, _)| text == "Display capture unavailable"));
    app.view = View::Logs;
    app.log_query = "LAYOUT".into();
    let output = frame(&mut app, &ctx, egui::vec2(900.0, 620.0), vec![]);
    let texts = text_rects(&output);
    for expected in [
        "WARN",
        "设置 timeout",
        "Device dropped: 9 (subscription total)",
        "Record fields truncated",
    ] {
        assert!(
            texts.iter().any(|(text, _, _)| text == expected),
            "missing {expected}"
        );
    }
    app.log_level = 1;
    let output = frame(&mut app, &ctx, egui::vec2(900.0, 620.0), vec![]);
    assert!(text_rects(&output)
        .iter()
        .any(|(text, _, _)| text == "No matching events"));
}
