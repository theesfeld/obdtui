//! Fighter / pilot HUD drawing — real vector strokes via egui painter.

use crate::feed::{ObdFeed, Telemetry};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use std::f32::consts::PI;

/// Phosphor palette
const BG: Color32 = Color32::from_rgb(2, 10, 6);
const GREEN: Color32 = Color32::from_rgb(0, 255, 100);
const GREEN_DIM: Color32 = Color32::from_rgb(0, 140, 60);
const AMBER: Color32 = Color32::from_rgb(255, 190, 40);
const RED: Color32 = Color32::from_rgb(255, 48, 48);
const CYAN: Color32 = Color32::from_rgb(40, 220, 255);

pub struct HudApp {
    feed: ObdFeed,
    /// Optional CRT-style scanline overlay
    scanlines: bool,
}

impl HudApp {
    pub fn new(feed: ObdFeed) -> Self {
        Self {
            feed,
            scanlines: true,
        }
    }
}

impl eframe::App for HudApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ~60 Hz redraw; OBD thread updates async.
        ctx.request_repaint_after(std::time::Duration::from_millis(16));

        let tele = self.feed.snapshot();

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let painter = ui.painter_at(rect);

                // Outer bezel
                painter.rect_stroke(
                    rect.shrink(6.0),
                    2.0,
                    Stroke::new(1.5_f32, GREEN_DIM),
                );

                // Header strip
                draw_header(&painter, rect, &tele);

                // Main instrument grid
                let body = Rect::from_min_max(
                    Pos2::new(rect.left() + 24.0, rect.top() + 48.0),
                    Pos2::new(rect.right() - 24.0, rect.bottom() - 36.0),
                );

                let cols = 3.0;
                let rows = 2.0;
                let cell_w = body.width() / cols;
                let cell_h = body.height() / rows;

                let rpm = get(&tele, "engine_rpm");
                let spd = get(&tele, "vehicle_speed");
                let cool = get(&tele, "coolant_temp");
                let thr = get(&tele, "throttle");
                let load = get(&tele, "engine_load");
                let iat = get(&tele, "intake_temp");
                let volt = get(&tele, "control_module_voltage");

                // Center reticle under gauges
                let center = body.center();
                draw_reticle(&painter, center, 28.0);

                // Gauges — real arcs
                let g = |c: u32, r: u32| {
                    Rect::from_min_size(
                        Pos2::new(body.left() + c as f32 * cell_w, body.top() + r as f32 * cell_h),
                        Vec2::new(cell_w, cell_h),
                    )
                    .shrink(12.0)
                };

                draw_arc_gauge(
                    &painter,
                    ArcSpec {
                        rect: g(0, 0),
                        label: "RPM",
                        value: rpm,
                        min: 0.0,
                        max: 7000.0,
                        unit: "rpm",
                        color: warn_rpm(rpm),
                    },
                );
                draw_arc_gauge(
                    &painter,
                    ArcSpec {
                        rect: g(1, 0),
                        label: "SPEED",
                        value: spd,
                        min: 0.0,
                        max: 200.0,
                        unit: "km/h",
                        color: GREEN,
                    },
                );
                draw_arc_gauge(
                    &painter,
                    ArcSpec {
                        rect: g(2, 0),
                        label: "COOLANT",
                        value: cool,
                        min: 40.0,
                        max: 120.0,
                        unit: "C",
                        color: warn_coolant(cool),
                    },
                );
                draw_arc_gauge(
                    &painter,
                    ArcSpec {
                        rect: g(0, 1),
                        label: "THROTTLE",
                        value: thr,
                        min: 0.0,
                        max: 100.0,
                        unit: "%",
                        color: CYAN,
                    },
                );
                draw_arc_gauge(
                    &painter,
                    ArcSpec {
                        rect: g(1, 1),
                        label: "LOAD",
                        value: load,
                        min: 0.0,
                        max: 100.0,
                        unit: "%",
                        color: AMBER,
                    },
                );
                draw_arc_gauge(
                    &painter,
                    ArcSpec {
                        rect: g(2, 1),
                        label: if volt > 0.5 { "VBATT" } else { "IAT" },
                        value: if volt > 0.5 { volt } else { iat },
                        min: if volt > 0.5 { 10.0 } else { -20.0 },
                        max: if volt > 0.5 { 16.0 } else { 80.0 },
                        unit: if volt > 0.5 { "V" } else { "C" },
                        color: if volt > 0.5 {
                            warn_volt(volt)
                        } else {
                            GREEN
                        },
                    },
                );

                // Footer
                draw_footer(&painter, rect, &tele);

                if self.scanlines {
                    draw_scanlines(&painter, rect);
                }

                // Click toggles scanlines
                let resp = ui.allocate_rect(rect, Sense::click());
                if resp.clicked() {
                    self.scanlines = !self.scanlines;
                }
            });
    }
}

fn get(t: &Telemetry, k: &str) -> f32 {
    t.values.get(k).copied().unwrap_or(0.0) as f32
}

fn warn_rpm(v: f32) -> Color32 {
    if v > 5500.0 {
        RED
    } else if v > 4500.0 {
        AMBER
    } else {
        GREEN
    }
}

fn warn_coolant(v: f32) -> Color32 {
    if v >= 110.0 {
        RED
    } else if v >= 100.0 {
        AMBER
    } else {
        GREEN
    }
}

fn warn_volt(v: f32) -> Color32 {
    if v < 11.5 || v > 15.2 {
        RED
    } else if v < 12.2 {
        AMBER
    } else {
        GREEN
    }
}

fn draw_header(painter: &egui::Painter, rect: Rect, tele: &Telemetry) {
    let vin = tele.vin.as_deref().unwrap_or("VIN ----");
    let status = &tele.status;
    painter.text(
        Pos2::new(rect.left() + 20.0, rect.top() + 14.0),
        Align2::LEFT_TOP,
        format!("OBD-MFD  ·  {vin}"),
        FontId::monospace(14.0),
        GREEN,
    );
    painter.text(
        Pos2::new(rect.right() - 20.0, rect.top() + 14.0),
        Align2::RIGHT_TOP,
        format!("{status}  ·  t={}", tele.ticks),
        FontId::monospace(12.0),
        GREEN_DIM,
    );
    if let Some(err) = &tele.error {
        painter.text(
            Pos2::new(rect.center().x, rect.top() + 30.0),
            Align2::CENTER_TOP,
            err,
            FontId::monospace(12.0),
            RED,
        );
    }
    // Horizon line
    painter.line_segment(
        [
            Pos2::new(rect.left() + 16.0, rect.top() + 40.0),
            Pos2::new(rect.right() - 16.0, rect.top() + 40.0),
        ],
        Stroke::new(1.0_f32, GREEN_DIM),
    );
}

fn draw_footer(painter: &egui::Painter, rect: Rect, tele: &Telemetry) {
    painter.line_segment(
        [
            Pos2::new(rect.left() + 16.0, rect.bottom() - 28.0),
            Pos2::new(rect.right() - 16.0, rect.bottom() - 28.0),
        ],
        Stroke::new(1.0_f32, GREEN_DIM),
    );
    painter.text(
        Pos2::new(rect.center().x, rect.bottom() - 18.0),
        Align2::CENTER_BOTTOM,
        "VECTOR HUD  ·  click = scanlines  ·  Esc/close window to quit",
        FontId::monospace(11.0),
        GREEN_DIM,
    );
    let _ = tele;
}

fn draw_reticle(painter: &egui::Painter, c: Pos2, r: f32) {
    painter.circle_stroke(c, r, Stroke::new(1.0_f32, GREEN_DIM));
    painter.line_segment(
        [Pos2::new(c.x - r - 8.0, c.y), Pos2::new(c.x - r + 4.0, c.y)],
        Stroke::new(1.0_f32, GREEN_DIM),
    );
    painter.line_segment(
        [Pos2::new(c.x + r - 4.0, c.y), Pos2::new(c.x + r + 8.0, c.y)],
        Stroke::new(1.0_f32, GREEN_DIM),
    );
    painter.line_segment(
        [Pos2::new(c.x, c.y - r - 8.0), Pos2::new(c.x, c.y - r + 4.0)],
        Stroke::new(1.0_f32, GREEN_DIM),
    );
    painter.line_segment(
        [Pos2::new(c.x, c.y + r - 4.0), Pos2::new(c.x, c.y + r + 8.0)],
        Stroke::new(1.0_f32, GREEN_DIM),
    );
}

struct ArcSpec<'a> {
    rect: Rect,
    label: &'a str,
    value: f32,
    min: f32,
    max: f32,
    unit: &'a str,
    color: Color32,
}

/// Draw a circular arc gauge with needle — true vector strokes.
fn draw_arc_gauge(painter: &egui::Painter, g: ArcSpec<'_>) {
    let ArcSpec {
        rect,
        label,
        value,
        min,
        max,
        unit,
        color,
    } = g;
    if rect.width() < 40.0 || rect.height() < 40.0 {
        return;
    }
    let c = rect.center();
    let radius = rect.width().min(rect.height()) * 0.38;

    stroke_arc(
        painter,
        c,
        radius,
        PI,
        0.0,
        Stroke::new(2.0_f32, GREEN_DIM),
        48,
    );

    let frac = if max > min {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let end = PI - frac * PI;
    stroke_arc(
        painter,
        c,
        radius,
        PI,
        end,
        Stroke::new(3.0_f32, color),
        48,
    );

    for i in 0..=10 {
        let t = i as f32 / 10.0;
        let ang = PI - t * PI;
        let outer = polar(c, radius + 2.0, ang);
        let inner = polar(c, radius - if i % 5 == 0 { 10.0 } else { 5.0 }, ang);
        painter.line_segment(
            [inner, outer],
            Stroke::new(1.0_f32, if i % 5 == 0 { GREEN } else { GREEN_DIM }),
        );
    }

    let nang = PI - frac * PI;
    let tip = polar(c, radius - 6.0, nang);
    let base_l = polar(c, 8.0, nang + 0.35);
    let base_r = polar(c, 8.0, nang - 0.35);
    painter.line_segment([base_l, tip], Stroke::new(2.0_f32, color));
    painter.line_segment([base_r, tip], Stroke::new(2.0_f32, color));
    painter.circle_filled(c, 3.5, color);

    painter.text(
        Pos2::new(c.x, rect.top() + 4.0),
        Align2::CENTER_TOP,
        label,
        FontId::monospace(13.0),
        GREEN_DIM,
    );
    painter.text(
        Pos2::new(c.x, c.y + radius * 0.55),
        Align2::CENTER_TOP,
        format!("{value:.1} {unit}"),
        FontId::monospace(16.0),
        color,
    );
}

fn polar(c: Pos2, r: f32, ang: f32) -> Pos2 {
    Pos2::new(c.x + r * ang.cos(), c.y - r * ang.sin())
}

/// Stroke a circular arc from angle `a0` to `a1` (radians, standard math angles).
fn stroke_arc(
    painter: &egui::Painter,
    c: Pos2,
    r: f32,
    a0: f32,
    a1: f32,
    stroke: Stroke,
    segments: usize,
) {
    if segments == 0 {
        return;
    }
    let mut prev = polar(c, r, a0);
    for i in 1..=segments {
        let t = i as f32 / segments as f32;
        let a = a0 + (a1 - a0) * t;
        let p = polar(c, r, a);
        painter.line_segment([prev, p], stroke);
        prev = p;
    }
}

fn draw_scanlines(painter: &egui::Painter, rect: Rect) {
    let mut y = rect.top();
    while y < rect.bottom() {
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0, 0, 0, 35)),
        );
        y += 3.0;
    }
}

// silence unused import warning path for Ui if any
#[allow(dead_code)]
fn _ui_type(_: &Ui) {}
