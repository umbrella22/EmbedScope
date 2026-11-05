//! Native inspection contract with the published rs_lvgl_ui API.

#[cfg(all(test, feature = "composition"))]
mod composition;

#[cfg(test)]
mod tests {
    use core::ptr::NonNull;
    use escope_lvgl::{native::NativeTree, UiBridge};
    use escope_protocol::{decode_payload, encode_payload, TreeQuery, UiNode};
    use rs_lvgl_ui::prelude::*;

    pub(super) type Node = (u32, u32, String, i32, i32, u32, u32, u32);

    pub(super) fn snapshot<const N: usize>(
        bridge: &mut UiBridge<NativeTree<N>>,
    ) -> (Vec<Node>, TreeQuery) {
        let mut query = TreeQuery::default();
        let mut nodes = Vec::new();
        loop {
            let mut request = [0; 64];
            let n = encode_payload(&query, &mut request).unwrap();
            let mut response = [0; 1024];
            let n = bridge.tree(&request[..n], &mut response).unwrap();
            let node: UiNode<'_> = decode_payload(&response[..n]).unwrap();
            assert!(node.name.is_empty());
            nodes.push((
                node.id,
                node.parent,
                node.kind.to_string(),
                node.x,
                node.y,
                node.width,
                node.height,
                node.flags,
            ));
            query.revision = node.revision;
            let Some(next) = node.next_cursor else { break };
            query.cursor = next;
        }
        (nodes, query)
    }

    #[derive(Clone)]
    enum CounterAction {
        Increment,
    }

    #[derive(Default)]
    struct Counter(u32);

    impl App for Counter {
        type Action = CounterAction;

        fn update(&mut self, _: CounterAction) {
            self.0 += 1;
        }

        fn view(&self, _: &Theme) -> View<CounterAction> {
            column([
                label(format!("Count: {}", self.0)),
                row([button("Increment").on_click(CounterAction::Increment)])
                    .height(px(48))
                    .width(px(200)),
            ])
            .width(px(320))
            .height(px(240))
            .padding(16)
            .gap(12)
        }
    }

    #[test]
    fn inspects_flat_style_views_with_one_native_provider() {
        rs_lvgl::init();
        let display = rs_lvgl::Display::register::<_, { 320 * 16 }>(320, 240, |_| {}).unwrap();
        let mut app = Counter::default();
        let mut ui = Ui::new(&display, Theme::dark()).unwrap();
        ui.mount(&app).unwrap();
        ui.timer_handler();

        // SAFETY: this test owns the only LVGL instance. All tree reads and
        // mutations are synchronous on this thread; the bridge is dropped
        // before unmounting or deleting the display.
        unsafe {
            let screen = NonNull::new(rs_lvgl::sys::lv_screen_active()).unwrap();
            {
                let mut bridge = UiBridge::new(NativeTree::<32>::new(screen));
                let (nodes, query) = snapshot(&mut bridge);
                assert_eq!(nodes[0].2, "lv_obj");
                let column = nodes.iter().find(|n| n.2 == "column").unwrap();
                let row = nodes.iter().find(|n| n.2 == "row").unwrap();
                let button = nodes.iter().find(|n| n.2 == "button").unwrap();
                let label = nodes
                    .iter()
                    .find(|n| n.2 == "label" && n.1 == column.0)
                    .unwrap();
                // Ui owns a neutral host object between the screen and the
                // application's view. Inspect the native hierarchy in full.
                let host = nodes.iter().find(|n| n.0 == column.1).unwrap();
                assert_eq!((host.2.as_str(), host.1), ("lv_obj", nodes[0].0));
                assert_eq!((column.5, column.6), (320, 240));
                assert_eq!((label.3, label.4), (16, 16));
                assert_eq!((row.1, row.5, row.6), (column.0, 200, 48));
                assert_eq!(button.1, row.0);

                app.update(CounterAction::Increment);
                ui.update(&app).unwrap();
                bridge.source_mut().changed().unwrap();
                let mut request = [0; 64];
                let n = encode_payload(&query, &mut request).unwrap();
                assert_eq!(
                    bridge.tree(&request[..n], &mut [0; 1024]),
                    Err(escope_protocol::Status::Stale)
                );
            }
            ui.unmount();
            drop(ui);
            display.delete();
            rs_lvgl::deinit();
        }
    }
}
