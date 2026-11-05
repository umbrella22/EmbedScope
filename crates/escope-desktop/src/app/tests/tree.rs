use super::*;

fn object(id: u32, parent: u32, kind: &str) -> UiNode {
    UiNode {
        id,
        parent,
        kind: kind.into(),
        name: String::new(),
        x: 0,
        y: 0,
        width: 100,
        height: 100,
        flags: 0,
        revision: 1,
    }
}

#[test]
fn breadth_first_snapshots_render_descendants_next_to_their_parents() {
    let (mut app, sender, _) = fixture();
    let ctx = egui::Context::default();
    theme::apply(&ctx, true);
    connect(&sender, capability::UI_TREE);
    // NativeTree enumerates breadth first. The button belongs before the
    // following sibling row when presented as an object hierarchy.
    let nodes = vec![
        object(1, 0, "screen"),
        object(2, 1, "column"),
        object(3, 1, "row"),
        object(4, 2, "button"),
        object(5, 4, "label"),
    ];
    sender.send(Update::Snapshot(nodes, None, None)).unwrap();
    let size = egui::vec2(1380.0, 900.0);
    frame(&mut app, &ctx, size, vec![]);
    let output = frame(&mut app, &ctx, size, vec![]);
    let labels: Vec<_> = text_rects(&output)
        .into_iter()
        .filter(|(text, rect, _)| {
            rect.left() < 300.0
                && ["screen", "column", "row", "button", "label"].contains(&text.as_str())
        })
        .map(|(text, _, _)| text)
        .collect();
    assert_eq!(labels, ["screen", "column", "button", "label", "row"]);

    app.tree_query = "#5".into();
    let output = frame(&mut app, &ctx, size, vec![]);
    let texts = text_rects(&output);
    for expected in ["screen", "column", "button", "label"] {
        assert!(
            texts.iter().any(|(text, _, _)| text == expected),
            "missing ancestor {expected}"
        );
    }
    assert!(!texts.iter().any(|(text, _, _)| text == "row"));
}

#[test]
fn display_selection_obeys_sibling_paint_order_across_nested_branches() {
    let nodes = vec![
        object(1, 0, "screen"),
        object(2, 1, "column"),
        object(3, 1, "button"),
        object(4, 2, "label"),
    ];
    // The later sibling is painted over the earlier column and its label.
    assert_eq!(super::super::inspector::node_at(&nodes, 20, 20), Some(3));
}
