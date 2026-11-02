//! Resolution-independent line icons in a shared 24-unit coordinate system.
use eframe::egui::{self, Color32, Pos2, Rect, Stroke};

#[derive(Clone, Copy)]
pub enum Icon {
    Inspect,
    Logs,
    Metrics,
    Device,
    Connect,
    Disconnect,
    Refresh,
    Sun,
    Moon,
    Play,
    Pause,
    Tap,
    Save,
    Close,
    Layers,
    Check,
    Book,
}

pub fn paint(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let unit = rect.width().min(rect.height()) / 24.0;
    let origin = rect.center() - egui::vec2(12.0, 12.0) * unit;
    let point = |x: f32, y: f32| origin + egui::vec2(x, y) * unit;
    let stroke = Stroke::new(1.65 * unit, color);
    let line = |points: &[[f32; 2]]| {
        painter.add(egui::Shape::line(
            points.iter().map(|p| point(p[0], p[1])).collect(),
            stroke,
        ));
    };
    let box_outline = |x, y, w, h, radius| {
        painter.rect_stroke(
            Rect::from_min_size(point(x, y), egui::vec2(w, h) * unit),
            radius,
            stroke,
            egui::StrokeKind::Middle,
        );
    };
    match icon {
        Icon::Inspect => {
            line(&[
                [5.0, 3.0],
                [6.0, 18.0],
                [10.0, 14.0],
                [14.0, 21.0],
                [17.0, 19.0],
                [13.0, 12.0],
                [19.0, 11.0],
                [5.0, 3.0],
            ]);
        }
        Icon::Logs => {
            box_outline(3.0, 4.0, 18.0, 16.0, 3);
            line(&[[7.0, 9.0], [10.0, 12.0], [7.0, 15.0]]);
            line(&[[13.0, 15.0], [17.0, 15.0]]);
        }
        Icon::Metrics => {
            line(&[
                [3.0, 13.0],
                [7.0, 13.0],
                [10.0, 5.0],
                [14.0, 20.0],
                [17.0, 10.0],
                [19.0, 13.0],
                [22.0, 13.0],
            ]);
        }
        Icon::Device => {
            box_outline(6.0, 6.0, 12.0, 12.0, 2);
            box_outline(9.0, 9.0, 6.0, 6.0, 1);
            for position in [8.0, 12.0, 16.0] {
                line(&[[position, 2.0], [position, 6.0]]);
                line(&[[position, 18.0], [position, 22.0]]);
                line(&[[2.0, position], [6.0, position]]);
                line(&[[18.0, position], [22.0, position]]);
            }
        }
        Icon::Connect | Icon::Disconnect => {
            line(&[[8.0, 3.0], [8.0, 8.0]]);
            line(&[[16.0, 3.0], [16.0, 8.0]]);
            line(&[[5.0, 8.0], [19.0, 8.0]]);
            line(&[
                [7.0, 8.0],
                [7.0, 13.0],
                [10.0, 16.0],
                [14.0, 16.0],
                [17.0, 13.0],
                [17.0, 8.0],
            ]);
            line(&[[12.0, 16.0], [12.0, 22.0]]);
            if matches!(icon, Icon::Disconnect) {
                line(&[[3.0, 3.0], [21.0, 21.0]]);
            }
        }
        Icon::Refresh => {
            line(&[
                [20.0, 10.0],
                [18.0, 6.0],
                [14.0, 4.0],
                [9.0, 4.0],
                [5.0, 7.0],
                [3.0, 11.0],
            ]);
            line(&[[20.0, 4.0], [20.0, 10.0], [14.0, 10.0]]);
            line(&[
                [4.0, 14.0],
                [6.0, 18.0],
                [10.0, 20.0],
                [15.0, 20.0],
                [19.0, 17.0],
                [21.0, 13.0],
            ]);
            line(&[[4.0, 20.0], [4.0, 14.0], [10.0, 14.0]]);
        }
        Icon::Sun => {
            painter.circle_stroke(point(12.0, 12.0), 4.0 * unit, stroke);
            for (dx, dy) in [(0.0, 1.0), (1.0, 0.0), (0.707, 0.707), (0.707, -0.707)] {
                for sign in [-1.0, 1.0] {
                    let a = point(12.0 + dx * 8.0 * sign, 12.0 + dy * 8.0 * sign);
                    let b = point(12.0 + dx * 10.5 * sign, 12.0 + dy * 10.5 * sign);
                    painter.line_segment([a, b], stroke);
                }
            }
        }
        Icon::Moon => {
            line(&[
                [18.0, 16.0],
                [13.0, 16.0],
                [9.0, 13.0],
                [8.0, 8.0],
                [9.0, 3.0],
                [5.0, 6.0],
                [3.0, 11.0],
                [4.0, 16.0],
                [8.0, 20.0],
                [13.0, 21.0],
                [18.0, 19.0],
                [21.0, 15.0],
                [18.0, 16.0],
            ]);
        }
        Icon::Play => line(&[[8.0, 4.0], [20.0, 12.0], [8.0, 20.0], [8.0, 4.0]]),
        Icon::Pause => {
            line(&[[8.0, 5.0], [8.0, 19.0]]);
            line(&[[16.0, 5.0], [16.0, 19.0]]);
        }
        Icon::Tap => {
            painter.circle_stroke(point(12.0, 12.0), 6.0 * unit, stroke);
            for points in [
                [[12.0, 2.0], [12.0, 7.0]],
                [[12.0, 17.0], [12.0, 22.0]],
                [[2.0, 12.0], [7.0, 12.0]],
                [[17.0, 12.0], [22.0, 12.0]],
            ] {
                line(&points);
            }
        }
        Icon::Save => {
            line(&[[12.0, 3.0], [12.0, 15.0]]);
            line(&[[7.0, 10.0], [12.0, 15.0], [17.0, 10.0]]);
            line(&[[4.0, 15.0], [4.0, 20.0], [20.0, 20.0], [20.0, 15.0]]);
        }
        Icon::Close => {
            line(&[[6.0, 6.0], [18.0, 18.0]]);
            line(&[[18.0, 6.0], [6.0, 18.0]]);
        }
        Icon::Layers => {
            line(&[
                [3.0, 8.0],
                [12.0, 3.0],
                [21.0, 8.0],
                [12.0, 13.0],
                [3.0, 8.0],
            ]);
            line(&[[3.0, 13.0], [12.0, 18.0], [21.0, 13.0]]);
            line(&[[3.0, 18.0], [12.0, 23.0], [21.0, 18.0]]);
        }
        Icon::Check => line(&[[5.0, 12.0], [10.0, 17.0], [20.0, 6.0]]),
        Icon::Book => {
            box_outline(5.0, 3.0, 15.0, 18.0, 2);
            line(&[[9.0, 3.0], [9.0, 21.0]]);
            line(&[[12.0, 8.0], [17.0, 8.0]]);
            line(&[[12.0, 12.0], [17.0, 12.0]]);
        }
    }
}

pub fn logo(painter: &egui::Painter, center: Pos2, size: f32, color: Color32) {
    let rect = Rect::from_center_size(center, egui::Vec2::splat(size));
    painter.rect_stroke(
        rect,
        7,
        Stroke::new(1.5_f32, color.gamma_multiply(0.5)),
        egui::StrokeKind::Inside,
    );
    let at = |x, y| rect.min + egui::vec2(x, y) * size;
    for segment in [
        [[0.67, 0.28], [0.32, 0.28]],
        [[0.32, 0.28], [0.32, 0.72]],
        [[0.32, 0.50], [0.62, 0.50]],
        [[0.32, 0.72], [0.67, 0.72]],
    ] {
        painter.line_segment(
            [
                at(segment[0][0], segment[0][1]),
                at(segment[1][0], segment[1][1]),
            ],
            Stroke::new(1.8_f32, color),
        );
    }
}
