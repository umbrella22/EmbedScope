//! Inspect the native result of typed composition and controlled updates.
use super::tests::snapshot;
use core::ptr::NonNull;
use escope_lvgl::{native::NativeTree, UiBridge};
use escope_protocol::{encode_payload, Status};
use rs_lvgl::sys;
use rs_lvgl_ui::prelude::*;

#[derive(Clone)]
enum SettingsAction {
    Speed(i32),
    Enabled(bool),
    Reset,
}

struct Settings {
    speed: i32,
    enabled: bool,
}

impl App for Settings {
    type Action = SettingsAction;

    fn update(&mut self, action: SettingsAction) {
        match action {
            SettingsAction::Speed(value) if value <= 50 => self.speed = value,
            SettingsAction::Speed(_) => {}
            SettingsAction::Enabled(value) => self.enabled = value,
            SettingsAction::Reset => self.speed = 30,
        }
    }

    fn view(&self, theme: &Theme) -> View<SettingsAction> {
        use components::{list::ListItem, panel::Panel};
        let controls = typed::column([])
            .child(
                ListItem::new("Fan")
                    .leading(typed::label("Mode"))
                    .content(typed::label("Fan power"))
                    .trailing(
                        typed::switch(binding(self.enabled, SettingsAction::Enabled))
                            .key(Key::Named("enabled")),
                    )
                    .view(theme),
            )
            .child(
                typed::slider(0, 100, binding(self.speed, SettingsAction::Speed))
                    .key(Key::Named("speed"))
                    .width(px(180)),
            )
            .child_if(self.enabled, || typed::label("Enabled"))
            .padding(12)
            .gap(12)
            .into_view();
        Panel::new("Settings")
            .header_action(typed::button("Reset").on_click(SettingsAction::Reset))
            .content(controls)
            .view(theme)
            .width(px(320))
            .height(px(240))
    }

    fn overlay(&self, theme: &Theme) -> Option<View<SettingsAction>> {
        Some(
            components::dialog::Dialog::new("Device")
                .content(typed::label("Controlled form"))
                .button("Reset", SettingsAction::Reset)
                .view(theme),
        )
    }
}

unsafe extern "C" {
    static lv_slider_class: sys::lv_obj_class_t;
    static lv_switch_class: sys::lv_obj_class_t;
}

/// Locate the native control independently of Inspector IDs or View keys.
unsafe fn widget(
    root: *mut sys::lv_obj_t,
    class: *const sys::lv_obj_class_t,
) -> *mut sys::lv_obj_t {
    let mut pending = vec![root];
    while let Some(object) = pending.pop() {
        // SAFETY: the caller owns this live native tree on the UI thread.
        unsafe {
            if sys::lv_obj_check_type(object, class) {
                return object;
            }
            for index in 0..sys::lv_obj_get_child_count(object) {
                pending.push(sys::lv_obj_get_child(object, index as i32));
            }
        }
    }
    panic!("control missing from the live tree");
}

#[test]
fn inspects_slots_bindings_and_conditional_children_after_native_events() {
    rs_lvgl::init();
    let display = rs_lvgl::Display::register::<_, { 320 * 16 }>(320, 240, |_| {}).unwrap();
    let mut settings = Settings {
        speed: 30,
        enabled: false,
    };
    let mut ui = Ui::new(&display, Theme::dark()).unwrap();
    ui.mount(&settings).unwrap();
    ui.timer_handler();

    // SAFETY: all reads, events and reconciliation are synchronous on this
    // test's UI thread. Both bridges are dropped before UI/display teardown.
    unsafe {
        let screen = NonNull::new(sys::lv_screen_active()).unwrap();
        {
            let mut bridge = UiBridge::new(NativeTree::<128>::new(screen));
            let (before, old_query) = snapshot(&mut bridge);
            for kind in ["slider", "switch", "button", "row", "column"] {
                assert!(before.iter().any(|node| node.2 == kind), "missing {kind}");
            }
            let switch_before = before.iter().find(|node| node.2 == "switch").unwrap();
            assert_eq!(switch_before.7 & sys::lv_state_t_LV_STATE_CHECKED, 0);
            let slider = widget(screen.as_ptr(), &raw const lv_slider_class);
            let switch = widget(screen.as_ptr(), &raw const lv_switch_class);
            sys::lv_obj_add_state(switch, sys::lv_state_t_LV_STATE_CHECKED);
            sys::lv_obj_send_event(
                switch,
                sys::lv_event_code_t_LV_EVENT_VALUE_CHANGED,
                core::ptr::null_mut(),
            );
            sys::lv_slider_set_value(slider, 80, false);
            sys::lv_obj_send_event(
                slider,
                sys::lv_event_code_t_LV_EVENT_VALUE_CHANGED,
                core::ptr::null_mut(),
            );
            assert_eq!(ui.drain(&mut settings, 8).unwrap().processed, 2);
            assert!(settings.enabled);
            assert_eq!(settings.speed, 30, "application rejected the native value");
            assert_eq!(
                sys::lv_slider_get_value(slider),
                30,
                "reconciliation corrected the control"
            );
            assert_eq!(
                ui.drain(&mut settings, 8).unwrap().processed,
                0,
                "correction did not echo an event"
            );
            ui.timer_handler();
            bridge.source_mut().changed().unwrap();
            let mut request = [0; 64];
            let n = encode_payload(&old_query, &mut request).unwrap();
            assert_eq!(
                bridge.tree(&request[..n], &mut [0; 1024]),
                Err(Status::Stale)
            );
            let (after, _) = snapshot(&mut bridge);
            assert_eq!(
                after.len(),
                before.len() + 1,
                "child_if added one native label"
            );
            let state = after.iter().find(|node| node.2 == "switch").unwrap();
            assert_ne!(state.7 & sys::lv_state_t_LV_STATE_CHECKED, 0);
            assert_eq!(widget(screen.as_ptr(), &raw const lv_slider_class), slider);
            assert_eq!(widget(screen.as_ptr(), &raw const lv_switch_class), switch);

            // LVGL's top layer is a separate native root, selected explicitly
            // by the integration when inspecting application overlays.
            let top = NonNull::new(sys::lv_layer_top()).unwrap();
            let mut overlay = UiBridge::new(NativeTree::<64>::new(top));
            let (dialog, _) = snapshot(&mut overlay);
            assert!(dialog.iter().any(|node| node.2 == "button"));
            assert!(dialog.iter().any(|node| node.2 == "column"));
            assert!(dialog.iter().all(|node| node.5 > 0 && node.6 > 0));
        }
        ui.unmount();
        drop(ui);
        display.delete();
        rs_lvgl::deinit();
    }
}
