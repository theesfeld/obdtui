//! Vector-style gauge drawing for ratatui (F2 + F6 MFD shell).
//!
//! Drawing is panic-safe: all coordinates are clamped to the buffer area.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;
use std::f64::consts::PI;

/// One vector instrument (arc gauge).
#[derive(Debug, Clone)]
pub struct ArcGauge {
    pub label: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub unit: String,
    pub accent: Color,
}

impl ArcGauge {
    pub fn new(
        label: impl Into<String>,
        value: f64,
        min: f64,
        max: f64,
        unit: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            value: sanitize_f64(value),
            min,
            max,
            unit: unit.into(),
            accent: Color::Cyan,
        }
    }

    fn frac(&self) -> f64 {
        if !self.value.is_finite() || self.max <= self.min {
            return 0.0;
        }
        ((self.value - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
    }
}

fn sanitize_f64(v: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

fn put(buf: &mut Buffer, area: Rect, x: i32, y: i32, ch: char, style: Style) {
    if x < area.x as i32
        || y < area.y as i32
        || x >= (area.x + area.width) as i32
        || y >= (area.y + area.height) as i32
    {
        return;
    }
    // Prefer ASCII-safe symbols (width 1) to avoid terminal panic/crash.
    if let Some(cell) = buf.cell_mut((x as u16, y as u16)) {
        cell.set_char(ch).set_style(style);
    }
}

impl Widget for ArcGauge {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 8 || area.height < 4 {
            return;
        }
        let cx = area.x + area.width / 2;
        let cy = area.y + area.height.saturating_sub(2);
        let radius = (area.width.min(area.height.saturating_mul(2)) / 2).saturating_sub(1) as i32;
        if radius < 2 {
            return;
        }
        let frac = self.frac();

        let steps = 48;
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let ang = PI - t * PI;
            let x = cx as i32 + (radius as f64 * ang.cos()).round() as i32;
            let y = cy as i32 - (radius as f64 * ang.sin() * 0.55).round() as i32;
            let style = if t <= frac {
                Style::default().fg(self.accent)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let ch = if t <= frac { '#' } else { '.' };
            put(buf, area, x, y, ch, style);
        }

        let nang = PI - frac * PI;
        let nx = cx as i32 + ((radius - 1) as f64 * nang.cos()).round() as i32;
        let ny = cy as i32 - ((radius - 1) as f64 * nang.sin() * 0.55).round() as i32;
        put(buf, area, nx, ny, '*', Style::default().fg(Color::Yellow));

        let val = format!("{:.1} {}", self.value, self.unit);
        let lx = area.x + area.width.saturating_sub(val.len() as u16) / 2;
        let ly = area.y + area.height.saturating_sub(1);
        for (i, ch) in val.chars().enumerate() {
            put(
                buf,
                area,
                lx as i32 + i as i32,
                ly as i32,
                ch,
                Style::default().fg(Color::White),
            );
        }
        let lab = &self.label;
        let llx = area.x + area.width.saturating_sub(lab.len() as u16) / 2;
        for (i, ch) in lab.chars().enumerate() {
            put(
                buf,
                area,
                llx as i32 + i as i32,
                area.y as i32,
                ch,
                Style::default().fg(Color::Gray),
            );
        }
    }
}

/// Horizontal tape gauge (F-16 style linear).
#[derive(Debug, Clone)]
pub struct TapeGauge {
    pub label: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub unit: String,
}

impl Widget for TapeGauge {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 6 || area.height < 2 {
            return;
        }
        let value = sanitize_f64(self.value);
        let frac = if self.max > self.min {
            ((value - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let inner_w = area.width.saturating_sub(2);
        let fill = ((inner_w as f64) * frac).round() as u16;
        let y = area.y + area.height / 2;
        for x in 0..area.width {
            let ch = if x == 0 || x == area.width - 1 {
                '|'
            } else if x.saturating_sub(1) < fill {
                '='
            } else {
                '-'
            };
            let color = if x.saturating_sub(1) < fill {
                Color::Cyan
            } else {
                Color::DarkGray
            };
            put(
                buf,
                area,
                (area.x + x) as i32,
                y as i32,
                ch,
                Style::default().fg(color),
            );
        }
        let text = format!("{} {:.1}{}", self.label, value, self.unit);
        for (i, ch) in text.chars().take(area.width as usize).enumerate() {
            put(
                buf,
                area,
                (area.x + i as u16) as i32,
                area.y as i32,
                ch,
                Style::default().fg(Color::Gray),
            );
        }
    }
}

/// Build default dashboard gauge specs from signal map.
pub fn dashboard_gauges(signals: &std::collections::HashMap<String, f64>) -> Vec<ArcGauge> {
    let get = |k: &str| sanitize_f64(signals.get(k).copied().unwrap_or(0.0));
    vec![
        ArcGauge::new("RPM", get("engine_rpm"), 0.0, 7000.0, "rpm"),
        ArcGauge::new("SPD", get("vehicle_speed"), 0.0, 200.0, "km/h"),
        ArcGauge {
            label: "COOL".into(),
            value: get("coolant_temp"),
            min: 40.0,
            max: 120.0,
            unit: "C".into(),
            accent: Color::Red,
        },
        ArcGauge::new("THR", get("throttle"), 0.0, 100.0, "%"),
        ArcGauge::new("LOAD", get("engine_load"), 0.0, 100.0, "%"),
        ArcGauge {
            label: "VBat".into(),
            value: get("control_module_voltage"),
            min: 10.0,
            max: 16.0,
            unit: "V".into(),
            accent: Color::Green,
        },
    ]
}
