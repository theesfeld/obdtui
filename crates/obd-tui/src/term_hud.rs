//! Terminal VECTOR HUD — Bresenham lines via ratatui Braille Canvas.
//!
//! Same idea as LOGO / Apple II hi-res: plot points, connect them. Braille
//! gives ~2×4 sub-pixels per cell so lines look like actual vectors.

use crate::app::App;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::symbols::Marker;
use ratatui::widgets::canvas::{Canvas, Circle, Context, Line as CLine};
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;
use std::f64::consts::PI;
use std::time::{SystemTime, UNIX_EPOCH};

const GREEN: Color = Color::Green;
const GREEN_DIM: Color = Color::DarkGray;
const AMBER: Color = Color::Yellow;
const RED: Color = Color::Red;
const CYAN: Color = Color::Cyan;

/// World coords: x -100..100, y -100..100 (center 0,0; +y up).
pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let map = app.signal_map();
    let rpm = map.get("engine_rpm").copied().unwrap_or(0.0);
    let spd = map.get("vehicle_speed").copied().unwrap_or(0.0);
    let thr = map.get("throttle").copied().unwrap_or(0.0);
    let load = map.get("engine_load").copied().unwrap_or(0.0);
    let cool = map.get("coolant_temp").copied().unwrap_or(0.0);

    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    // Synthetic pitch/roll from OBD (no IMU) — still draws real lines.
    let pitch = ((thr * 0.25) + (load * 0.12) - 8.0).clamp(-20.0, 25.0);
    let roll = (thr - 50.0) * 0.03;

    let vin = app.session.vin.as_deref().unwrap_or("NO VIN");
    let title = format!(
        " VECTOR HUD  ·  {vin}  ·  RPM {rpm:.0}  SPD {spd:.0}  THR {thr:.0}%  COOL {cool:.0}C "
    );

    let canvas = Canvas::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(ratatui::style::Style::default().fg(GREEN))
                .title(title),
        )
        .marker(Marker::Braille)
        .x_bounds([-100.0, 100.0])
        .y_bounds([-100.0, 100.0])
        .paint(move |ctx| {
            draw_fov(ctx);
            draw_horizon_ladder(ctx, pitch, roll);
            draw_gun_cross(ctx);
            draw_flight_path(ctx, thr);
            draw_v_tape(
                ctx,
                -92.0,
                "SPD",
                spd,
                0.0,
                160.0,
                tape_col(spd, 120.0, 140.0),
            );
            draw_v_tape(
                ctx,
                92.0,
                "RPM",
                rpm,
                0.0,
                7000.0,
                tape_col(rpm, 4500.0, 5500.0),
            );
            draw_aoa(ctx, thr);
            draw_heading(ctx, (t * 8.0 + spd * 0.5) % 360.0);
            draw_radar(ctx, 0.0, -62.0, 28.0, t, load, rpm);
            // status ticks
            line(ctx, -98.0, 98.0, 98.0, 98.0, GREEN_DIM);
            line(ctx, -98.0, -98.0, 98.0, -98.0, GREEN_DIM);
        });

    f.render_widget(canvas, area);
}

fn tape_col(v: f64, warn: f64, red: f64) -> Color {
    if v >= red {
        RED
    } else if v >= warn {
        AMBER
    } else {
        GREEN
    }
}

fn line(ctx: &mut Context, x1: f64, y1: f64, x2: f64, y2: f64, color: Color) {
    ctx.draw(&CLine {
        x1,
        y1,
        x2,
        y2,
        color,
    });
}

fn rot(x: f64, y: f64, cx: f64, cy: f64, roll_deg: f64) -> (f64, f64) {
    let a = roll_deg.to_radians();
    let (s, c) = (a.sin(), a.cos());
    let dx = x - cx;
    let dy = y - cy;
    (cx + dx * c - dy * s, cy + dx * s + dy * c)
}

fn draw_fov(ctx: &mut Context) {
    let l = 12.0;
    // corners
    for (x, y, sx, sy) in [
        (-95.0, 95.0, 1.0, -1.0),
        (95.0, 95.0, -1.0, -1.0),
        (-95.0, -95.0, 1.0, 1.0),
        (95.0, -95.0, -1.0, 1.0),
    ] {
        line(ctx, x, y, x + sx * l, y, GREEN_DIM);
        line(ctx, x, y, x, y + sy * l, GREEN_DIM);
    }
}

fn draw_horizon_ladder(ctx: &mut Context, pitch: f64, roll: f64) {
    let cy = pitch * 1.8; // move ladder with pitch
                          // horizon
    let (x1, y1) = rot(-55.0, cy, 0.0, 0.0, roll);
    let (x2, y2) = rot(55.0, cy, 0.0, 0.0, roll);
    line(ctx, x1, y1, x2, y2, GREEN);

    for step in (-20..=20).step_by(5) {
        if step == 0 {
            continue;
        }
        let y = cy - step as f64 * 2.2;
        if !(-70.0..=70.0).contains(&y) {
            continue;
        }
        let half = if step % 10 == 0 { 28.0 } else { 16.0 };
        let gap = 10.0;
        let col = if step > 0 { GREEN } else { GREEN_DIM };
        let (a0x, a0y) = rot(-half, y, 0.0, 0.0, roll);
        let (a1x, a1y) = rot(-gap, y, 0.0, 0.0, roll);
        let (b0x, b0y) = rot(gap, y, 0.0, 0.0, roll);
        let (b1x, b1y) = rot(half, y, 0.0, 0.0, roll);
        line(ctx, a0x, a0y, a1x, a1y, col);
        line(ctx, b0x, b0y, b1x, b1y, col);
        // end ticks
        let tick = if step > 0 { -4.0 } else { 4.0 };
        let (t0x, t0y) = rot(-half, y, 0.0, 0.0, roll);
        let (t1x, t1y) = rot(-half, y + tick, 0.0, 0.0, roll);
        let (t2x, t2y) = rot(half, y, 0.0, 0.0, roll);
        let (t3x, t3y) = rot(half, y + tick, 0.0, 0.0, roll);
        line(ctx, t0x, t0y, t1x, t1y, col);
        line(ctx, t2x, t2y, t3x, t3y, col);
    }
}

fn draw_gun_cross(ctx: &mut Context) {
    line(ctx, -14.0, 0.0, -5.0, 0.0, GREEN);
    line(ctx, 5.0, 0.0, 14.0, 0.0, GREEN);
    line(ctx, 0.0, 8.0, 0.0, 3.0, GREEN);
    // wings
    line(ctx, -18.0, -8.0, -6.0, -8.0, GREEN);
    line(ctx, 6.0, -8.0, 18.0, -8.0, GREEN);
    line(ctx, -6.0, -8.0, 0.0, -3.0, GREEN);
    line(ctx, 6.0, -8.0, 0.0, -3.0, GREEN);
}

fn draw_flight_path(ctx: &mut Context, thr: f64) {
    let y = (thr - 30.0) * 0.25;
    ctx.draw(&Circle {
        x: 0.0,
        y,
        radius: 6.0,
        color: CYAN,
    });
    line(ctx, -16.0, y, -8.0, y, CYAN);
    line(ctx, 8.0, y, 16.0, y, CYAN);
    line(ctx, 0.0, y + 10.0, 0.0, y + 7.0, CYAN);
}

fn draw_v_tape(
    ctx: &mut Context,
    x_side: f64,
    _label: &str,
    value: f64,
    min: f64,
    max: f64,
    color: Color,
) {
    let left = x_side < 0.0;
    let x0 = if left { -88.0 } else { 72.0 };
    let x1 = if left { -72.0 } else { 88.0 };
    // box
    line(ctx, x0, 45.0, x1, 45.0, GREEN_DIM);
    line(ctx, x0, -45.0, x1, -45.0, GREEN_DIM);
    line(ctx, x0, 45.0, x0, -45.0, GREEN_DIM);
    line(ctx, x1, 45.0, x1, -45.0, GREEN_DIM);
    // caret window
    line(ctx, x0 + 1.0, 6.0, x1 - 1.0, 6.0, color);
    line(ctx, x0 + 1.0, -6.0, x1 - 1.0, -6.0, color);
    line(ctx, x0 + 1.0, 6.0, x0 + 1.0, -6.0, color);
    line(ctx, x1 - 1.0, 6.0, x1 - 1.0, -6.0, color);

    // ticks relative to value
    let range = (max - min).max(1.0);
    let scale = 70.0 / (range * 0.4);
    for i in -6..=6 {
        let v = (value / 10.0).floor() * 10.0 + i as f64 * 10.0;
        if v < min || v > max {
            continue;
        }
        let y = (v - value) * scale;
        if !(-40.0..=40.0).contains(&y) || y.abs() < 7.0 {
            continue;
        }
        let major = (v as i64).rem_euclid(50) == 0;
        let tw = if major { 6.0 } else { 3.0 };
        let mx = (x0 + x1) / 2.0;
        line(
            ctx,
            mx - tw,
            y,
            mx + tw,
            y,
            if major { GREEN } else { GREEN_DIM },
        );
    }
}

fn draw_aoa(ctx: &mut Context, thr: f64) {
    let y = (thr - 40.0) * 0.4;
    let x = -42.0;
    let col = if thr > 85.0 { AMBER } else { GREEN };
    line(ctx, x, y - 10.0, x + 6.0, y, col);
    line(ctx, x, y + 10.0, x + 6.0, y, col);
    line(ctx, x, y - 10.0, x, y + 10.0, col);
}

fn draw_heading(ctx: &mut Context, hdg: f64) {
    let y = -48.0;
    line(ctx, -50.0, y, 50.0, y, GREEN_DIM);
    // caret
    line(ctx, 0.0, y + 6.0, -4.0, y + 1.0, GREEN);
    line(ctx, 0.0, y + 6.0, 4.0, y + 1.0, GREEN);
    let px = 1.8;
    for d in -30..=30 {
        let h = (hdg + d as f64).rem_euclid(360.0);
        let x = d as f64 * px;
        let major = (h as i64).rem_euclid(10) == 0;
        let th = if major { 5.0 } else { 2.5 };
        line(ctx, x, y, x, y - th, if major { GREEN } else { GREEN_DIM });
    }
}

fn draw_radar(ctx: &mut Context, cx: f64, cy: f64, r: f64, t: f64, load: f64, rpm: f64) {
    // range rings (polyline circles)
    for ring in 1..=3 {
        let rr = r * (ring as f64 / 3.0);
        let segs = 36;
        for s in 0..segs {
            let a0 = (s as f64 / segs as f64) * PI * 2.0;
            let a1 = ((s + 1) as f64 / segs as f64) * PI * 2.0;
            line(
                ctx,
                cx + rr * a0.cos(),
                cy + rr * a0.sin(),
                cx + rr * a1.cos(),
                cy + rr * a1.sin(),
                GREEN_DIM,
            );
        }
    }
    line(ctx, cx - r, cy, cx + r, cy, GREEN_DIM);
    line(ctx, cx, cy - r, cx, cy + r, GREEN_DIM);

    // sweep
    let sweep = t * 2.2;
    line(
        ctx,
        cx,
        cy,
        cx + r * sweep.cos(),
        cy + r * sweep.sin(),
        GREEN,
    );
    for k in 1..6 {
        let a = sweep - k as f64 * 0.12;
        line(
            ctx,
            cx + (r * 0.25) * a.cos(),
            cy + (r * 0.25) * a.sin(),
            cx + r * a.cos(),
            cy + r * a.sin(),
            GREEN_DIM,
        );
    }

    // blips from load/rpm
    let n = 1 + (load / 30.0) as i32;
    for i in 0..n {
        let a = sweep - 0.5 - i as f64 * 0.7 + rpm * 0.0002;
        let br = (r * (0.4 + i as f64 * 0.15)).min(r * 0.9);
        let bx = cx + br * a.cos();
        let by = cy + br * a.sin();
        ctx.draw(&Circle {
            x: bx,
            y: by,
            radius: 2.0,
            color: GREEN,
        });
        line(ctx, bx - 3.0, by, bx + 3.0, by, GREEN);
    }
}
