//! F-16 style VECTOR HUD — pure line symbology (not consumer gauges).
//!
//! Pitch ladder, velocity vector, airspeed/power tapes, radar PPI, reticle.
//! All strokes via egui painter (true 2D vectors).

use crate::feed::{ObdFeed, Telemetry};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use std::f32::consts::PI;
use std::time::Instant;

const BG: Color32 = Color32::from_rgb(0, 0, 0);
const G: Color32 = Color32::from_rgb(0, 255, 70);
const G_DIM: Color32 = Color32::from_rgb(0, 120, 45);
const G_FAINT: Color32 = Color32::from_rgb(0, 70, 30);
const AMBER: Color32 = Color32::from_rgb(255, 200, 40);
const RED: Color32 = Color32::from_rgb(255, 40, 40);

pub struct HudApp {
    feed: ObdFeed,
    t0: Instant,
    scanlines: bool,
}

impl HudApp {
    pub fn new(feed: ObdFeed) -> Self {
        Self {
            feed,
            t0: Instant::now(),
            scanlines: false,
        }
    }
}

impl eframe::App for HudApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
        let tele = self.feed.snapshot();
        let t = self.t0.elapsed().as_secs_f32();

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let p = ui.painter_at(rect);

                // Outer HUD FOV brackets (vector corners)
                draw_fov_brackets(&p, rect.shrink(10.0), G_DIM);

                let rpm = get(&tele, "engine_rpm");
                let spd = get(&tele, "vehicle_speed");
                let cool = get(&tele, "coolant_temp");
                let thr = get(&tele, "throttle");
                let load = get(&tele, "engine_load");
                let iat = get(&tele, "intake_temp");
                let volt = get(&tele, "control_module_voltage");

                // Synthetic pitch (deg): throttle/load drive a flight-path feel.
                // Not IMU pitch — OBD stand-in for HUD motion.
                let pitch = ((thr * 0.25) + (load * 0.15) - 8.0).clamp(-20.0, 25.0);
                // Synthetic roll: small oscillation from throttle changes + idle
                let roll = (thr - 50.0) * 0.04 + (t * 0.3).sin() * 0.4;

                let c = rect.center();
                // Keep symbology in classic HUD glass region
                let glass =
                    Rect::from_center_size(c, Vec2::new(rect.width() * 0.72, rect.height() * 0.78));

                // --- VECTOR LAYERS ---
                draw_horizon_and_pitch_ladder(&p, glass, pitch, roll);
                draw_velocity_vector(&p, glass, pitch, thr);
                draw_flight_path_marker(&p, glass.center(), thr);
                draw_gun_cross(&p, glass.center());

                // Side tapes (vector boxes + ticks)
                draw_v_tape(
                    &p,
                    TapeSpec {
                        rect: Rect::from_min_size(
                            Pos2::new(glass.left() + 8.0, glass.top() + glass.height() * 0.18),
                            Vec2::new(72.0, glass.height() * 0.55),
                        ),
                        label: "SPD",
                        value: spd,
                        min: 0.0,
                        max: 160.0,
                        unit: "km/h",
                        color: tape_color(spd, 0.0, 120.0, 140.0),
                    },
                );
                draw_v_tape(
                    &p,
                    TapeSpec {
                        rect: Rect::from_min_size(
                            Pos2::new(glass.right() - 80.0, glass.top() + glass.height() * 0.18),
                            Vec2::new(72.0, glass.height() * 0.55),
                        ),
                        label: "RPM",
                        value: rpm,
                        min: 0.0,
                        max: 7000.0,
                        unit: "",
                        color: tape_color(rpm, 0.0, 4500.0, 5500.0),
                    },
                );

                // AoA / throttle bracket (left of VV)
                draw_aoa_bracket(&p, glass.center(), thr);

                // Bottom heading tape (synthetic heading from time+speed for motion)
                let hdg = ((t * 3.0 + spd * 0.5) % 360.0 + 360.0) % 360.0;
                draw_heading_tape(&p, glass, hdg);

                // Radar / sonar PPI — lower center, pure vector rings + sweep
                let radar_c = Pos2::new(glass.center().x, glass.bottom() - glass.height() * 0.16);
                let radar_r = glass.height() * 0.13;
                draw_radar_ppi(&p, radar_c, radar_r, t, load, rpm);

                // Side status vector boxes
                draw_data_block(
                    &p,
                    Pos2::new(glass.left() + 12.0, glass.top() + 8.0),
                    "MASTER",
                    &[
                        ("MODE", "OBD-HUD".into()),
                        ("BUS", "HS-CAN".into()),
                        ("LINK", short_status(&tele.status)),
                    ],
                    G,
                );
                draw_data_block(
                    &p,
                    Pos2::new(glass.right() - 140.0, glass.top() + 8.0),
                    "SYS",
                    &[
                        ("COOL", format!("{cool:.0} C")),
                        ("IAT", format!("{iat:.0} C")),
                        ("LOAD", format!("{load:.0} %")),
                        (
                            "VBAT",
                            if volt > 0.5 {
                                format!("{volt:.1} V")
                            } else {
                                "—".into()
                            },
                        ),
                    ],
                    warn_sys(cool, volt),
                );

                // VIN / ticks
                p.text(
                    Pos2::new(rect.center().x, rect.top() + 12.0),
                    Align2::CENTER_TOP,
                    format!("HUD  ·  {}", tele.vin.as_deref().unwrap_or("NO VIN")),
                    FontId::monospace(13.0),
                    G,
                );
                if let Some(err) = &tele.error {
                    p.text(
                        Pos2::new(rect.center().x, rect.bottom() - 14.0),
                        Align2::CENTER_BOTTOM,
                        err,
                        FontId::monospace(12.0),
                        RED,
                    );
                } else {
                    p.text(
                        Pos2::new(rect.center().x, rect.bottom() - 12.0),
                        Align2::CENTER_BOTTOM,
                        format!(
                            "VECTOR  ·  t={}  ·  click=scan  ·  close window to exit",
                            tele.ticks
                        ),
                        FontId::monospace(11.0),
                        G_FAINT,
                    );
                }

                if self.scanlines {
                    draw_scanlines(&p, rect);
                }

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

fn short_status(s: &str) -> String {
    if s.len() > 18 {
        s.chars().take(18).collect()
    } else {
        s.to_string()
    }
}

fn tape_color(v: f32, _lo: f32, warn: f32, red: f32) -> Color32 {
    if v >= red {
        RED
    } else if v >= warn {
        AMBER
    } else {
        G
    }
}

fn warn_sys(cool: f32, volt: f32) -> Color32 {
    if cool >= 110.0 || (volt > 0.5 && !(11.5..=15.2).contains(&volt)) {
        RED
    } else if cool >= 100.0 || (volt > 0.5 && volt < 12.2) {
        AMBER
    } else {
        G
    }
}

// ─── Vector primitives ───────────────────────────────────────────────────────

fn stroke(p: &egui::Painter, a: Pos2, b: Pos2, w: f32, c: Color32) {
    p.line_segment([a, b], Stroke::new(w, c));
}

fn polar(c: Pos2, r: f32, ang: f32) -> Pos2 {
    // ang: 0 = right, CCW; HUD pitch uses degrees with 0 level
    Pos2::new(c.x + r * ang.cos(), c.y + r * ang.sin())
}

fn rot(p: Pos2, c: Pos2, roll_rad: f32) -> Pos2 {
    let d = p - c;
    let (s, co) = roll_rad.sin_cos();
    Pos2::new(c.x + d.x * co - d.y * s, c.y + d.x * s + d.y * co)
}

fn draw_fov_brackets(p: &egui::Painter, r: Rect, c: Color32) {
    let len = 28.0;
    let w = 1.2_f32;
    // four corners
    for (ox, oy) in [
        (r.left(), r.top()),
        (r.right(), r.top()),
        (r.left(), r.bottom()),
        (r.right(), r.bottom()),
    ] {
        let sx = if ox == r.left() { 1.0 } else { -1.0 };
        let sy = if oy == r.top() { 1.0 } else { -1.0 };
        stroke(p, Pos2::new(ox, oy), Pos2::new(ox + sx * len, oy), w, c);
        stroke(p, Pos2::new(ox, oy), Pos2::new(ox, oy + sy * len), w, c);
    }
}

/// Artificial horizon + pitch ladder (classic F-16 style bars).
fn draw_horizon_and_pitch_ladder(p: &egui::Painter, glass: Rect, pitch_deg: f32, roll_deg: f32) {
    let c = glass.center();
    let roll = roll_deg.to_radians();
    let px_per_deg = glass.height() / 50.0; // visual scale

    // Horizon line (level reference, rolled)
    let hl = 90.0;
    let h0 = rot(Pos2::new(c.x - hl, c.y + pitch_deg * px_per_deg), c, roll);
    let h1 = rot(Pos2::new(c.x + hl, c.y + pitch_deg * px_per_deg), c, roll);
    stroke(p, h0, h1, 1.5_f32, G);

    // Pitch lines every 5°
    for step in (-20_i32..=20).step_by(5) {
        if step == 0 {
            continue;
        }
        let y = c.y + (pitch_deg - step as f32) * px_per_deg;
        if y < glass.top() + 40.0 || y > glass.bottom() - 80.0 {
            continue;
        }
        let half = if step % 10 == 0 { 36.0 } else { 22.0 };
        let gap = 14.0; // gap at center
        let a0 = rot(Pos2::new(c.x - half, y), c, roll);
        let a1 = rot(Pos2::new(c.x - gap, y), c, roll);
        let b0 = rot(Pos2::new(c.x + gap, y), c, roll);
        let b1 = rot(Pos2::new(c.x + half, y), c, roll);
        let col = if step > 0 { G } else { G_DIM };
        stroke(p, a0, a1, 1.0_f32, col);
        stroke(p, b0, b1, 1.0_f32, col);
        // positive pitch: inverted U ticks; negative: open
        if step > 0 {
            let u0 = rot(Pos2::new(c.x - half, y), c, roll);
            let u1 = rot(Pos2::new(c.x - half, y + 6.0), c, roll);
            let u2 = rot(Pos2::new(c.x + half, y), c, roll);
            let u3 = rot(Pos2::new(c.x + half, y + 6.0), c, roll);
            stroke(p, u0, u1, 1.0_f32, col);
            stroke(p, u2, u3, 1.0_f32, col);
        } else {
            let d0 = rot(Pos2::new(c.x - half, y), c, roll);
            let d1 = rot(Pos2::new(c.x - half, y - 6.0), c, roll);
            let d2 = rot(Pos2::new(c.x + half, y), c, roll);
            let d3 = rot(Pos2::new(c.x + half, y - 6.0), c, roll);
            stroke(p, d0, d1, 1.0_f32, col);
            stroke(p, d2, d3, 1.0_f32, col);
        }
        // numeric
        let label = format!("{}", step.abs());
        let lp = rot(Pos2::new(c.x - half - 14.0, y), c, roll);
        p.text(
            lp,
            Align2::RIGHT_CENTER,
            label,
            FontId::monospace(10.0),
            col,
        );
    }
}

/// Waterline / aircraft reference (fixed).
fn draw_gun_cross(p: &egui::Painter, c: Pos2) {
    let w = 1.2_f32;
    // boresight cross — small
    stroke(
        p,
        Pos2::new(c.x - 18.0, c.y),
        Pos2::new(c.x - 6.0, c.y),
        w,
        G,
    );
    stroke(
        p,
        Pos2::new(c.x + 6.0, c.y),
        Pos2::new(c.x + 18.0, c.y),
        w,
        G,
    );
    stroke(
        p,
        Pos2::new(c.x, c.y - 10.0),
        Pos2::new(c.x, c.y - 4.0),
        w,
        G,
    );
    // wing line under
    stroke(
        p,
        Pos2::new(c.x - 22.0, c.y + 10.0),
        Pos2::new(c.x - 8.0, c.y + 10.0),
        w,
        G,
    );
    stroke(
        p,
        Pos2::new(c.x + 8.0, c.y + 10.0),
        Pos2::new(c.x + 22.0, c.y + 10.0),
        w,
        G,
    );
    stroke(
        p,
        Pos2::new(c.x - 8.0, c.y + 10.0),
        Pos2::new(c.x, c.y + 4.0),
        w,
        G,
    );
    stroke(
        p,
        Pos2::new(c.x + 8.0, c.y + 10.0),
        Pos2::new(c.x, c.y + 4.0),
        w,
        G,
    );
}

/// Flight path marker (velocity vector) — classic circle + wings.
fn draw_flight_path_marker(p: &egui::Painter, c: Pos2, thr: f32) {
    // slight offset from center based on throttle (energy)
    let y = c.y - (thr - 30.0) * 0.35;
    let fp = Pos2::new(c.x, y);
    p.circle_stroke(fp, 9.0, Stroke::new(1.4_f32, G));
    stroke(
        p,
        Pos2::new(fp.x - 22.0, fp.y),
        Pos2::new(fp.x - 11.0, fp.y),
        1.4_f32,
        G,
    );
    stroke(
        p,
        Pos2::new(fp.x + 11.0, fp.y),
        Pos2::new(fp.x + 22.0, fp.y),
        1.4_f32,
        G,
    );
    stroke(
        p,
        Pos2::new(fp.x, fp.y - 16.0),
        Pos2::new(fp.x, fp.y - 11.0),
        1.4_f32,
        G,
    );
}

fn draw_velocity_vector(p: &egui::Painter, glass: Rect, pitch: f32, thr: f32) {
    // ghost VV trail (vector ticks along path)
    let c = glass.center();
    let y = c.y + pitch * 2.0 - thr * 0.2;
    for i in 1..4 {
        let t = i as f32;
        let pt = Pos2::new(c.x + t * 14.0, y + t * 2.0);
        if glass.contains(pt) {
            p.circle_stroke(pt, 2.0, Stroke::new(1.0_f32, G_FAINT));
        }
    }
}

struct TapeSpec<'a> {
    rect: Rect,
    label: &'a str,
    value: f32,
    min: f32,
    max: f32,
    unit: &'a str,
    color: Color32,
}

/// Vertical tape with caret (F-16 airspeed/altitude style).
fn draw_v_tape(p: &egui::Painter, spec: TapeSpec<'_>) {
    let TapeSpec {
        rect,
        label,
        value,
        min,
        max,
        unit,
        color,
    } = spec;
    if rect.height() < 40.0 {
        return;
    }
    let left = rect.left();
    let right = rect.right();
    let mid_x = rect.center().x;
    // box outline
    p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, G_DIM));
    // center caret window
    let win = Rect::from_center_size(rect.center(), Vec2::new(rect.width() - 4.0, 22.0));
    p.rect_stroke(win, 0.0, Stroke::new(1.5_f32, color));
    p.text(
        win.center(),
        Align2::CENTER_CENTER,
        format!("{value:.0}"),
        FontId::monospace(14.0),
        color,
    );

    // tick ladder scrolling with value
    let range = (max - min).max(1.0);
    let px_per_unit = rect.height() / (range * 0.45);
    for i in -8..=8 {
        let v = (value / 10.0).floor() * 10.0 + i as f32 * 10.0;
        if v < min || v > max {
            continue;
        }
        let y = rect.center().y - (v - value) * px_per_unit;
        if y < rect.top() + 4.0 || y > rect.bottom() - 4.0 {
            continue;
        }
        if (y - rect.center().y).abs() < 12.0 {
            continue; // under caret
        }
        let major = (v as i32).rem_euclid(50) == 0;
        let tick = if major { 14.0 } else { 8.0 };
        stroke(
            p,
            Pos2::new(mid_x - tick * 0.5, y),
            Pos2::new(mid_x + tick * 0.5, y),
            1.0_f32,
            if major { G } else { G_DIM },
        );
        if major {
            p.text(
                Pos2::new(left + 4.0, y),
                Align2::LEFT_CENTER,
                format!("{v:.0}"),
                FontId::monospace(9.0),
                G_DIM,
            );
        }
    }

    p.text(
        Pos2::new(rect.center().x, rect.top() - 2.0),
        Align2::CENTER_BOTTOM,
        label,
        FontId::monospace(11.0),
        G_DIM,
    );
    if !unit.is_empty() {
        p.text(
            Pos2::new(rect.center().x, rect.bottom() + 2.0),
            Align2::CENTER_TOP,
            unit,
            FontId::monospace(9.0),
            G_FAINT,
        );
    }
    let _ = (left, right);
}

fn draw_aoa_bracket(p: &egui::Painter, c: Pos2, thr: f32) {
    // left of center — AoA-like caret driven by throttle
    let x = c.x - 48.0;
    let y = c.y - (thr - 40.0) * 0.8;
    let col = if thr > 85.0 { AMBER } else { G };
    stroke(
        p,
        Pos2::new(x, y - 12.0),
        Pos2::new(x + 8.0, y),
        1.2_f32,
        col,
    );
    stroke(
        p,
        Pos2::new(x, y + 12.0),
        Pos2::new(x + 8.0, y),
        1.2_f32,
        col,
    );
    stroke(
        p,
        Pos2::new(x, y - 12.0),
        Pos2::new(x, y + 12.0),
        1.2_f32,
        col,
    );
}

fn draw_heading_tape(p: &egui::Painter, glass: Rect, hdg: f32) {
    let y = glass.bottom() - glass.height() * 0.30;
    let mid = glass.center().x;
    let half_w = 120.0;
    stroke(
        p,
        Pos2::new(mid - half_w, y),
        Pos2::new(mid + half_w, y),
        1.0_f32,
        G_DIM,
    );
    // caret
    stroke(
        p,
        Pos2::new(mid, y - 10.0),
        Pos2::new(mid - 5.0, y - 2.0),
        1.2_f32,
        G,
    );
    stroke(
        p,
        Pos2::new(mid, y - 10.0),
        Pos2::new(mid + 5.0, y - 2.0),
        1.2_f32,
        G,
    );
    p.text(
        Pos2::new(mid, y - 14.0),
        Align2::CENTER_BOTTOM,
        format!("HDG {:03.0}", hdg),
        FontId::monospace(12.0),
        G,
    );

    let px_per_deg = 2.2;
    for d in -60..=60 {
        let h = (hdg + d as f32).rem_euclid(360.0);
        let x = mid + d as f32 * px_per_deg;
        if x < mid - half_w || x > mid + half_w {
            continue;
        }
        let major = (h as i32).rem_euclid(10) == 0;
        let tick_h = if major { 8.0 } else { 4.0 };
        stroke(
            p,
            Pos2::new(x, y),
            Pos2::new(x, y + tick_h),
            1.0_f32,
            if major { G } else { G_FAINT },
        );
        if major && (h as i32).rem_euclid(30) == 0 {
            p.text(
                Pos2::new(x, y + 10.0),
                Align2::CENTER_TOP,
                format!("{:03.0}", h),
                FontId::monospace(9.0),
                G_DIM,
            );
        }
    }
}

/// Radar/sonar PPI: range rings + rotating sweep + return blips from load/rpm.
fn draw_radar_ppi(p: &egui::Painter, c: Pos2, r: f32, t: f32, load: f32, rpm: f32) {
    // range rings
    for i in 1..=3 {
        let rr = r * (i as f32 / 3.0);
        // polyline circle
        let segs = 48;
        let mut prev = polar(c, rr, 0.0);
        for s in 1..=segs {
            let a = (s as f32 / segs as f32) * PI * 2.0;
            let pt = polar(c, rr, a);
            stroke(p, prev, pt, 1.0_f32, G_FAINT);
            prev = pt;
        }
    }
    // cross
    stroke(
        p,
        Pos2::new(c.x - r, c.y),
        Pos2::new(c.x + r, c.y),
        1.0_f32,
        G_FAINT,
    );
    stroke(
        p,
        Pos2::new(c.x, c.y - r),
        Pos2::new(c.x, c.y + r),
        1.0_f32,
        G_FAINT,
    );

    // rotating sweep beam
    let sweep = t * 1.8;
    let tip = polar(c, r, sweep);
    stroke(p, c, tip, 1.5_f32, G);
    // fade trail
    for k in 1..8 {
        let a = sweep - k as f32 * 0.08;
        let a2 = a - 0.06;
        let p0 = polar(c, r * 0.2, a);
        let p1 = polar(c, r, a);
        let p2 = polar(c, r, a2);
        stroke(p, p0, p1, 1.0_f32, G_FAINT);
        stroke(p, p1, p2, 1.0_f32, G_FAINT);
    }

    // synthetic returns — intensity from load, radius from rpm
    let n_blips = 1 + (load / 25.0) as i32;
    for i in 0..n_blips {
        let a = sweep - 0.4 - i as f32 * 0.55 + (rpm * 0.0003);
        let br = r * (0.35 + (i as f32) * 0.15 + (load / 200.0));
        let bp = polar(c, br.min(r * 0.92), a);
        p.circle_stroke(bp, 3.0, Stroke::new(1.2_f32, G));
        // range tick
        stroke(
            p,
            Pos2::new(bp.x - 4.0, bp.y),
            Pos2::new(bp.x + 4.0, bp.y),
            1.0_f32,
            G,
        );
    }

    p.text(
        Pos2::new(c.x, c.y + r + 6.0),
        Align2::CENTER_TOP,
        "RDR/SNR",
        FontId::monospace(10.0),
        G_DIM,
    );
}

fn draw_data_block(
    p: &egui::Painter,
    origin: Pos2,
    title: &str,
    rows: &[(&str, String)],
    color: Color32,
) {
    let w = 130.0;
    let h = 18.0 + rows.len() as f32 * 14.0;
    let r = Rect::from_min_size(origin, Vec2::new(w, h));
    p.rect_stroke(r, 0.0, Stroke::new(1.0_f32, G_DIM));
    p.text(
        Pos2::new(origin.x + 4.0, origin.y + 2.0),
        Align2::LEFT_TOP,
        title,
        FontId::monospace(10.0),
        G_DIM,
    );
    for (i, (k, v)) in rows.iter().enumerate() {
        let y = origin.y + 16.0 + i as f32 * 14.0;
        p.text(
            Pos2::new(origin.x + 6.0, y),
            Align2::LEFT_TOP,
            *k,
            FontId::monospace(11.0),
            G_FAINT,
        );
        p.text(
            Pos2::new(origin.x + w - 6.0, y),
            Align2::RIGHT_TOP,
            v,
            FontId::monospace(11.0),
            color,
        );
    }
}

fn draw_scanlines(p: &egui::Painter, rect: Rect) {
    let mut y = rect.top();
    while y < rect.bottom() {
        stroke(
            p,
            Pos2::new(rect.left(), y),
            Pos2::new(rect.right(), y),
            1.0_f32,
            Color32::from_rgba_unmultiplied(0, 0, 0, 40),
        );
        y += 3.0;
    }
}
