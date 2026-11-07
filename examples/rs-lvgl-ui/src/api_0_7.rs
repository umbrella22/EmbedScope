//! Inspect renamed UI constructors through native objects and real pointer input.
use super::tests::snapshot;
use core::ptr::NonNull;
use escope_lvgl::{native::NativeTree, UiBridge};
use escope_protocol::{encode_payload, Status};
use rs_lvgl::input_device::{InputDriver, Pointer, PointerInputData};
use rs_lvgl::sys;
use rs_lvgl_ui::prelude::*;
use std::{cell::Cell, cell::RefCell, rc::Rc};

#[derive(Clone)]
enum Action {
    Advance,
}

struct Dashboard {
    progress: i32,
}

impl App for Dashboard {
    type Action = Action;

    fn update(&mut self, _: Action) {
        self.progress = 75;
    }

    fn view(&self, _: &Theme) -> View<Action> {
        static PIXEL: [u8; 2] = [0xff, 0xff];
        typed::column([])
            .child(
                typed::container()
                    .style(Style::plain())
                    .width(px(240))
                    .height(px(48))
                    .child(
                        typed::button("Advance")
                            .width(px(160))
                            .height(px(40))
                            .on_click(Action::Advance),
                    ),
            )
            .child(
                typed::progress_bar(0, 100, self.progress)
                    .width(px(240))
                    .height(px(16))
                    .progress_style(Style::plain().background(Color::rgb(30, 210, 120))),
            )
            .child(
                typed::slider(0, 100, self.progress)
                    .width(px(240))
                    .height(px(16))
                    .thumb_style(Style::plain().background(Color::rgb(230, 180, 30))),
            )
            .child(
                typed::message_box("Native UI 0.6")
                    .width(px(240))
                    .height(px(56)),
            )
            .child(typed::animated_image(
                [ImageSource::rgb565(1, 1, &PIXEL)],
                1000,
            ))
            .style(
                Style::plain()
                    .justify_content(FlexAlign::Start)
                    .align_items(ItemAlign::Start)
                    .align_content(FlexAlign::Start),
            )
            .width(px(320))
            .height(px(240))
            .padding(16)
            .gap(8)
            .scrollable(false)
            .into_view()
    }
}

#[test]
fn renamed_views_keep_native_kinds_and_respond_to_pointer_input() {
    rs_lvgl::init();
    let pixels = Rc::new(RefCell::new(vec![0u8; 320 * 240 * 3]));
    let output = pixels.clone();
    let display = rs_lvgl::Display::register::<_, { 320 * 16 }>(320, 240, move |refresh| {
        let mut frame = output.borrow_mut();
        let width = (refresh.area.x2 - refresh.area.x1 + 1) as usize;
        for y in refresh.area.y1..=refresh.area.y2 {
            for x in refresh.area.x1..=refresh.area.x2 {
                let source =
                    ((y - refresh.area.y1) as usize * width) + (x - refresh.area.x1) as usize;
                let target = (y as usize * 320 + x as usize) * 3;
                let color = refresh.colors[source];
                frame[target..target + 3].copy_from_slice(&[color.r(), color.g(), color.b()]);
            }
        }
    })
    .unwrap();
    let sample = Rc::new(Cell::new((0i32, 0i32, false)));
    let input = sample.clone();
    let pointer = Pointer::register(
        move || {
            let (x, y, pressed) = input.get();
            let point = PointerInputData::Touch(rs_lvgl::point::Point::new(x, y));
            if pressed {
                point.pressed()
            } else {
                point.released()
            }
            .once()
        },
        &display,
    )
    .unwrap();
    let mut dashboard = Dashboard { progress: 25 };
    let mut ui = Ui::new(&display, Theme::dark()).unwrap();
    ui.mount(&dashboard).unwrap();
    ui.timer_handler();

    // SAFETY: this test exclusively owns LVGL on one thread. Input, flush,
    // tree reads and reconciliation never overlap. Drop the bridge and input
    // before unmounting/deleting the display and deinitializing the provider.
    unsafe {
        sys::lv_refr_now(core::ptr::null_mut());
        let before_pixels = pixels.borrow().clone();
        assert!(before_pixels.iter().any(|channel| *channel != 0));
        {
            let root = NonNull::new(sys::lv_screen_active()).unwrap();
            let mut bridge = UiBridge::new(NativeTree::<64>::new(root));
            let (before, old_query) = snapshot(&mut bridge);
            for kind in ["bar", "msgbox", "animimage", "slider", "button", "column"] {
                assert!(before.iter().any(|node| node.2 == kind), "missing {kind}");
            }
            let bar = before.iter().find(|node| node.2 == "bar").unwrap();
            let bar_center =
                ((bar.4 as usize + bar.6 as usize / 2) * 320 + bar.3 as usize + bar.5 as usize / 2)
                    * 3;
            let button = before.iter().find(|node| node.2 == "button").unwrap();
            let container = before.iter().find(|node| node.0 == button.1).unwrap();
            assert_eq!(container.2, "lv_obj");
            assert_eq!(
                (container.3, container.4, container.5, container.6),
                (16, 16, 240, 48)
            );
            let point = (
                button.3 + button.5 as i32 / 2,
                button.4 + button.6 as i32 / 2,
            );
            sample.set((point.0, point.1, true));
            sys::lv_indev_read(pointer.get_descriptor().unwrap());
            sample.set((point.0, point.1, false));
            sys::lv_indev_read(pointer.get_descriptor().unwrap());
            assert_eq!(ui.drain(&mut dashboard, 8).unwrap().processed, 1);
            assert_eq!(dashboard.progress, 75);
            assert_eq!(ui.drain(&mut dashboard, 8).unwrap().processed, 0);
            ui.timer_handler();
            sys::lv_refr_now(core::ptr::null_mut());
            assert_ne!(
                before_pixels[bar_center..bar_center + 3],
                pixels.borrow()[bar_center..bar_center + 3],
                "the progress fill crosses the native bar's midpoint"
            );
            let changed_pixels = before_pixels
                .as_chunks::<3>()
                .0
                .iter()
                .zip(pixels.borrow().as_chunks::<3>().0)
                .filter(|(before, after)| before != after)
                .count();
            assert!(changed_pixels > 100, "the actual display must update");
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
                assert_eq!(
                    (old.0, old.1, &old.2, old.3, old.4, old.5, old.6),
                    (new.0, new.1, &new.2, new.3, new.4, new.5, new.6),
                    "progress changes preserve native geometry and kinds"
                );
            }
            println!(
                "NATIVE_UI_0_7_OK nodes={} changed_pixels={changed_pixels}",
                after.len()
            );
        }
        drop(pointer);
        ui.unmount();
        drop(ui);
        display.delete();
        rs_lvgl::deinit();
    }
}
