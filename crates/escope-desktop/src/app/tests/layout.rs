use super::*;

#[test]
fn long_object_ids_keep_their_own_column() {
    let (mut app, sender, _) = fixture();
    let ctx = egui::Context::default();
    crate::fonts::install_system_cjk(&ctx);
    theme::apply(&ctx, true);
    connect(&sender, capability::UI_TREE);
    let mut object = node(1);
    object.id = u32::MAX;
    object.kind = "long object kind with enough text to reach the identifier".into();
    sender
        .send(Update::Snapshot(vec![object.clone()], None, None))
        .unwrap();
    let size = egui::vec2(900.0, 620.0);
    frame(&mut app, &ctx, size, vec![]);
    let output = frame(&mut app, &ctx, size, vec![]);
    let texts = text_rects(&output);
    let (_, id, clip) = texts
        .iter()
        .find(|(text, _, _)| text == "#4294967295")
        .unwrap();
    let (_, kind, kind_clip) = texts
        .iter()
        .find(|(text, _, _)| text == &object.kind)
        .unwrap();
    assert!(clip.contains_rect(*id), "entire identifier stays visible");
    assert!(
        id.left() - kind.intersect(*kind_clip).right() >= 6.0,
        "long labels cannot paint under the identifier"
    );
}

fn filled_rects(output: &egui::FullOutput, fill: egui::Color32) -> Vec<egui::Rect> {
    fn collect(shape: &egui::Shape, fill: egui::Color32, out: &mut Vec<egui::Rect>) {
        match shape {
            egui::Shape::Rect(shape) if shape.fill == fill => out.push(shape.rect),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, fill, out);
                }
            }
            _ => (),
        }
    }
    let mut result = Vec::new();
    for shape in &output.shapes {
        collect(&shape.shape, fill, &mut result);
    }
    result
}

fn click(app: &mut Inspector, ctx: &egui::Context, size: egui::Vec2, point: egui::Pos2) {
    for pressed in [true, false] {
        frame(
            app,
            ctx,
            size,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
}

#[test]
fn language_popup_keeps_text_insets_and_padded_rows_are_clickable() {
    for locale in LOCALES {
        for dark in [true, false] {
            for size in [egui::vec2(900.0, 620.0), egui::vec2(1380.0, 900.0)] {
                let (mut app, _, mut commands) = fixture();
                app.i18n = I18n::new(Some(locale.id), []);
                app.dark = dark;
                let ctx = egui::Context::default();
                crate::fonts::install_system_cjk(&ctx);
                theme::apply(&ctx, dark);
                ctx.style_mut_of(
                    if dark {
                        egui::Theme::Dark
                    } else {
                        egui::Theme::Light
                    },
                    |style| style.animation_time = 0.0,
                );
                frame(&mut app, &ctx, size, vec![]);
                // The globe is the third 40-point control up from the bottom of the rail.
                click(&mut app, &ctx, size, egui::pos2(32.0, size.y - 128.0));
                let output = frame(&mut app, &ctx, size, vec![]);
                let texts = text_rects(&output);
                let (_, selected_text, _) = texts
                    .iter()
                    .find(|(text, _, _)| text == locale.name)
                    .expect("language options visible after opening globe menu");
                let row = filled_rects(&output, Palette::new(dark).accent_bg)
                    .into_iter()
                    .find(|rect| rect.contains_rect(*selected_text))
                    .expect("selected language row");
                assert!(row.height() >= 40.0, "menu row is a complete hit target");
                assert!(
                    selected_text.left() - row.left() >= 10.0,
                    "text must be inset from selection fill"
                );
                assert!(
                    selected_text.top() - row.top() >= 6.0,
                    "text must clear the upper rounded edge"
                );
                assert!(
                    row.bottom() - selected_text.bottom() >= 6.0,
                    "text must clear the lower rounded edge"
                );
                for label in [
                    app.i18n.text("language-title"),
                    app.i18n.text("language-system"),
                    "English".into(),
                    "简体中文".into(),
                ] {
                    let (_, bounds, clip) = texts
                        .iter()
                        .find(|(text, _, _)| text == &label)
                        .expect("menu label");
                    assert!(clip.expand(1.0).contains_rect(*bounds), "{label} clipped");
                    assert!(
                        egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(*bounds)
                    );
                }
                let target = LOCALES.iter().find(|other| other.id != locale.id).unwrap();
                let (_, bounds, _) = texts
                    .iter()
                    .find(|(text, _, _)| text == target.name)
                    .unwrap();
                // Click the empty left inset, not the glyphs: the whole row must select.
                click(
                    &mut app,
                    &ctx,
                    size,
                    egui::pos2(bounds.left() - 8.0, bounds.center().y),
                );
                assert_eq!(app.i18n.locale().id, target.id);
                let output = frame(&mut app, &ctx, size, vec![]);
                assert!(
                    !text_rects(&output)
                        .iter()
                        .any(|(text, _, _)| text == &app.i18n.text("language-title")),
                    "choice closes popup"
                );
                assert!(
                    commands.try_recv().is_err(),
                    "language selection stays local"
                );
            }
        }
    }
}

#[test]
fn translated_log_controls_fit_both_themes_and_window_sizes() {
    for locale in LOCALES {
        for dark in [true, false] {
            for size in [egui::vec2(900.0, 620.0), egui::vec2(1380.0, 900.0)] {
                let (mut app, _, _) = fixture();
                app.i18n = I18n::new(Some(locale.id), []);
                app.dark = dark;
                app.view = View::Logs;
                app.log_level = 2;
                let ctx = egui::Context::default();
                crate::fonts::install_system_cjk(&ctx);
                theme::apply(&ctx, dark);
                ctx.style_mut_of(
                    if dark {
                        egui::Theme::Dark
                    } else {
                        egui::Theme::Light
                    },
                    |style| style.animation_time = 0.0,
                );
                frame(&mut app, &ctx, size, vec![]);
                let output = frame(&mut app, &ctx, size, vec![]);
                let texts = text_rects(&output);
                for label in [
                    app.i18n.text("level-warn"),
                    app.i18n.text("follow"),
                    app.i18n.text("clear"),
                    app.i18n.text("log-filter"),
                ] {
                    let (_, bounds, clip) = texts
                        .iter()
                        .find(|(text, _, _)| text == &label)
                        .expect("toolbar label");
                    assert!(
                        clip.expand(1.0).contains_rect(*bounds),
                        "{label} clipped at {size:?}"
                    );
                    assert!(
                        egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(*bounds),
                        "{label} outside viewport"
                    );
                }
                let (_, combo_text, _) = texts
                    .iter()
                    .find(|(text, _, _)| text == &app.i18n.text("level-warn"))
                    .unwrap();
                click(&mut app, &ctx, size, combo_text.center());
                let output = frame(&mut app, &ctx, size, vec![]);
                let texts = text_rects(&output);
                let (_, bounds, _) = texts
                    .iter()
                    .find(|(text, bounds, _)| {
                        text == &app.i18n.text("level-info") && bounds.top() > combo_text.top()
                    })
                    .expect("log filter popup");
                click(
                    &mut app,
                    &ctx,
                    size,
                    egui::pos2(bounds.left() - 8.0, bounds.center().y),
                );
                assert_eq!(app.log_level, 3, "padded filter options select correctly");
            }
        }
    }
}
