//! Inspect 0.7 controls and retained resources through the native protocol tree.
use super::tests::snapshot;
use core::ptr::NonNull;
use escope_lvgl::{native::NativeTree, UiBridge};
use escope_protocol::{encode_payload, Status};
use rs_lvgl::{sys, NativeObject};
use rs_lvgl_ui::prelude::*;
use std::{cell::Cell, ffi::CStr, num::NonZeroU32, rc::Rc};

#[derive(Default)]
struct Hooks {
    creates: Cell<usize>,
    applies: Cell<usize>,
    ticks: Cell<usize>,
    deletes: Cell<usize>,
}

struct Gauge(Rc<Hooks>);
struct GaugeProps(i32);

impl CustomWidget for Gauge {
    type Props = GaugeProps;

    fn validate(props: &GaugeProps) -> Result<(), UiError> {
        if (0..=100).contains(&props.0) {
            Ok(())
        } else {
            Err(UiError::InvalidProperty)
        }
    }

    fn create(&mut self, parent: &mut Obj<'_>, _: &GaugeProps) -> Result<CustomObject, UiError> {
        // SAFETY: the mounted owner receives one live direct child; no pointer
        // or borrowed backing storage escapes this custom subtree.
        unsafe {
            let child = NonNull::new(sys::lv_arc_create(parent.raw().as_ptr()))
                .ok_or(UiError::Allocation)?;
            self.0.creates.set(self.0.creates.get() + 1);
            CustomObject::new(parent, child)
        }
    }

    fn apply_props(
        &mut self,
        object: &mut CustomObject,
        props: &GaugeProps,
    ) -> Result<(), UiError> {
        // SAFETY: this hook exclusively borrows the live arc on the UI thread.
        unsafe {
            sys::lv_arc_set_value(object.raw().as_ptr(), props.0);
        }
        self.0.applies.set(self.0.applies.get() + 1);
        Ok(())
    }

    fn tick(&mut self, _: &mut CustomObject, _: u32) -> Result<(), UiError> {
        self.0.ticks.set(self.0.ticks.get() + 1);
        Ok(())
    }

    fn delete(&mut self, _: &mut CustomObject) {
        self.0.deletes.set(self.0.deletes.get() + 1);
    }
}

#[derive(Clone)]
enum Action {
    Range(SliderRange),
    Text(String),
}

struct Dashboard {
    range: SliderRange,
    text: String,
    history: ChartHistory,
    value: i32,
    reverse: bool,
    hooks: Rc<Hooks>,
}

impl App for Dashboard {
    type Action = Action;

    fn update(&mut self, action: Action) {
        match action {
            Action::Range(range) => self.range = range,
            Action::Text(text) => self.text = text,
        }
    }

    fn view(&self, _: &Theme) -> View<Action> {
        row([
            typed::range_slider(0, 100, binding(self.range, Action::Range))
                .width(px(120))
                .height(px(20))
                .key(Key::Id(1))
                .into_view(),
            typed::textarea(binding(self.text.as_str(), |text: &str| {
                Action::Text(text.into())
            }))
            .accepted_chars("0123456789")
            .input_max_length(NonZeroU32::new(2))
            .cursor_position(Some(1))
            .cursor_click_position(false)
            .width(px(120))
            .height(px(40))
            .key(Key::Id(2))
            .into_view(),
            chart(ChartSpec::new(
                ChartType::Line,
                ChartAxes::new(ChartRange::new(0, 100), ChartRange::new(0, 100)),
                vec![ChartSeries::new(
                    77,
                    Color::rgb(20, 150, 200),
                    ChartData::History(self.history.snapshot()),
                )],
            ))
            .width(px(120))
            .height(px(72))
            .key(Key::Id(3)),
            custom(Gauge(self.hooks.clone()), GaugeProps(self.value))
                .width(px(120))
                .height(px(72))
                .key(Key::Id(4)),
        ])
        .width(px(288))
        .height(px(240))
        .padding(16)
        .gap(12)
        .flex_wrap(true)
        .flex_reverse(self.reverse)
    }
}

#[test]
fn inspects_depth_controls_updates_and_custom_props_without_virtual_nodes() {
    rs_lvgl::init();
    let flushes = Rc::new(Cell::new(0));
    let output = flushes.clone();
    let display = rs_lvgl::Display::register::<_, { 320 * 8 }>(320, 260, move |_| {
        output.set(output.get() + 1);
    })
    .unwrap();
    let hooks = Rc::new(Hooks::default());
    let mut dashboard = Dashboard {
        range: SliderRange::new(20, 70),
        text: "A中Z".into(),
        history: ChartHistory::new(1, NonZeroU32::new(4).unwrap(), ChartSampleKind::Y),
        value: 30,
        reverse: false,
        hooks: hooks.clone(),
    };
    dashboard.history.push_y(20).unwrap();
    let mut ui = Ui::new(&display, Theme::dark()).unwrap();
    ui.mount(&dashboard).unwrap();
    ui.timer_handler();

    // SAFETY: this test exclusively owns LVGL on one thread. All pointers are
    // borrowed while the mounted tree is live; the bridge is dropped before
    // unmount, display deletion and deinitialization. No native callback reenters Ui.
    unsafe {
        let screen = NonNull::new(sys::lv_screen_active()).unwrap();
        {
            let mut bridge = UiBridge::new(NativeTree::<32>::new(screen));
            let (before, old_query) = snapshot(&mut bridge);
            let row = before.iter().find(|node| node.2 == "row").unwrap();
            let children: Vec<_> = before.iter().filter(|node| node.1 == row.0).collect();
            assert_eq!(
                children
                    .iter()
                    .map(|node| node.2.as_str())
                    .collect::<Vec<_>>(),
                ["slider", "textarea", "chart", "arc"]
            );
            assert_eq!((row.5, row.6), (288, 240));
            // View keys, chart series IDs and custom Props create no tree nodes.
            // The textarea's internal label remains part of its native subtree.
            assert_eq!(before.len(), 8);
            assert!(before
                .iter()
                .any(|node| node.2 == "label" && node.1 == children[1].0));
            let host = sys::lv_obj_get_child(screen.as_ptr(), 0);
            let root = sys::lv_obj_get_child(host, 0);
            assert_eq!(
                sys::lv_obj_get_style_flex_flow(root, 0),
                sys::lv_flex_flow_t_LV_FLEX_FLOW_ROW_WRAP
            );
            let objects: Vec<_> = (0..4).map(|i| sys::lv_obj_get_child(root, i)).collect();
            assert!(objects.iter().all(|object| !object.is_null()));
            let (slider, textarea, plot, gauge) = (objects[0], objects[1], objects[2], objects[3]);
            assert_eq!(sys::lv_slider_get_left_value(slider), 20);
            assert_eq!(sys::lv_slider_get_value(slider), 70);
            assert_eq!(
                CStr::from_ptr(sys::lv_textarea_get_text(textarea))
                    .to_str()
                    .unwrap(),
                "A中Z"
            );
            assert_eq!(sys::lv_textarea_get_cursor_pos(textarea), 1);
            assert!(!sys::lv_textarea_get_cursor_click_pos(textarea));
            assert_eq!(sys::lv_arc_get_value(gauge), 30);
            let series = sys::lv_chart_get_series_next(plot, core::ptr::null());
            assert!(!series.is_null());
            assert!(sys::lv_chart_get_series_next(plot, series).is_null());
            assert_eq!(sys::lv_chart_get_point_count(plot), 4);
            assert_eq!(*sys::lv_chart_get_series_y_array(plot, series), 20);
            assert_eq!(
                (hooks.creates.get(), hooks.applies.get(), hooks.ticks.get()),
                (1, 1, 0)
            );

            dashboard.reverse = true;
            dashboard.value = 65;
            dashboard.text = "model中".into();
            dashboard.range = SliderRange::new(25, 80);
            dashboard.history.push_y(55).unwrap();
            ui.update(&dashboard).unwrap();
            ui.timer_handler();
            assert_eq!(
                sys::lv_obj_get_style_flex_flow(root, 0),
                sys::lv_flex_flow_t_LV_FLEX_FLOW_ROW_WRAP_REVERSE
            );
            assert_eq!(
                (0..4)
                    .map(|i| sys::lv_obj_get_child(root, i))
                    .collect::<Vec<_>>(),
                objects,
                "wrap/reverse and new Props preserve native child identities"
            );
            assert_eq!(
                sys::lv_chart_get_series_next(plot, core::ptr::null()),
                series
            );
            assert_eq!(
                core::slice::from_raw_parts(sys::lv_chart_get_series_y_array(plot, series), 2),
                &[20, 55]
            );
            assert_eq!(sys::lv_slider_get_left_value(slider), 25);
            assert_eq!(sys::lv_slider_get_value(slider), 80);
            assert_eq!(sys::lv_arc_get_value(gauge), 65);
            assert_eq!(
                CStr::from_ptr(sys::lv_textarea_get_text(textarea))
                    .to_str()
                    .unwrap(),
                "model中"
            );
            assert_eq!(hooks.applies.get(), 2);
            assert_eq!(hooks.ticks.get(), 0);
            ui.tick(16).unwrap();
            assert_eq!(hooks.ticks.get(), 1);
            assert_eq!(hooks.applies.get(), 2);

            // Real native edits use the controlled range and text event paths.
            sys::lv_slider_set_start_value(slider, 35, false);
            sys::lv_slider_set_value(slider, 90, false);
            sys::lv_obj_send_event(
                slider,
                sys::lv_event_code_t_LV_EVENT_VALUE_CHANGED,
                core::ptr::null_mut(),
            );
            // Unlike slider setters, textarea set_text emits VALUE_CHANGED.
            sys::lv_textarea_set_text(textarea, c"42".as_ptr());
            assert_eq!(ui.drain(&mut dashboard, 8).unwrap().processed, 2);
            assert_eq!(dashboard.range, SliderRange::new(35, 90));
            assert_eq!(dashboard.text, "42");

            bridge.source_mut().changed().unwrap();
            let mut request = [0; 64];
            let n = encode_payload(&old_query, &mut request).unwrap();
            assert_eq!(
                bridge.tree(&request[..n], &mut [0; 1024]),
                Err(Status::Stale)
            );
            let (after, _) = snapshot(&mut bridge);
            assert_eq!(before.len(), after.len());
            for (old, new) in before.iter().zip(&after) {
                assert_eq!((old.0, old.1, &old.2), (new.0, new.1, &new.2));
            }
            let reversed_slider = after.iter().find(|node| node.2 == "slider").unwrap();
            assert_ne!(
                (children[0].3, children[0].4),
                (reversed_slider.3, reversed_slider.4),
                "reverse changes geometry, not child order"
            );
            let native_objects = [
                screen.as_ptr(),
                host,
                root,
                slider,
                textarea,
                plot,
                gauge,
                sys::lv_textarea_get_label(textarea),
            ];
            for (node, object) in after.iter().zip(native_objects) {
                assert!(!object.is_null());
                let mut area: sys::lv_area_t = core::mem::zeroed();
                sys::lv_obj_get_coords(object, &mut area);
                assert_eq!((node.3, node.4), (area.x1, area.y1));
                assert_eq!(
                    (node.5, node.6),
                    (
                        (area.x2 - area.x1 + 1).max(0) as u32,
                        (area.y2 - area.y1 + 1).max(0) as u32
                    )
                );
            }
            sys::lv_refr_now(sys::lv_obj_get_display(root));
            assert!(flushes.get() > 0);
            println!(
                "NATIVE_DEPTH_0_7_OK nodes={} flushes={}",
                after.len(),
                flushes.get()
            );
        }
        ui.unmount();
        assert_eq!(hooks.deletes.get(), 1);
        assert_eq!(hooks.creates.get(), 1);
        drop(ui);
        display.delete();
        rs_lvgl::deinit();
    }
}
