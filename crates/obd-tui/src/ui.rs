//! Ratatui views: table, vector gauges, MFD shell.

use crate::app::{App, Tab};
use crate::gauges::{dashboard_gauges, ArcGauge, TapeGauge};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Tabs, Wrap};
use ratatui::Frame;

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(f.area());

    draw_header(f, chunks[0], app);
    match app.tab {
        Tab::Live => draw_live(f, chunks[1], app),
        Tab::Gauges => draw_gauges(f, chunks[1], app),
        Tab::Mfd => draw_mfd(f, chunks[1], app),
        Tab::Dtc => draw_dtc(f, chunks[1], app),
        Tab::Modules => draw_modules(f, chunks[1], app),
        Tab::Log => draw_log(f, chunks[1], app),
        Tab::Help => draw_help(f, chunks[1], app),
    }
    draw_footer(f, chunks[2], app);
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let titles = [
        "1 Live",
        "2 Gauges",
        "3 MFD",
        "4 DTC",
        "5 Modules",
        "6 Log",
        "7 Help",
    ]
    .iter()
    .cloned()
    .map(Line::from)
    .collect::<Vec<_>>();
    let idx = match app.tab {
        Tab::Live => 0,
        Tab::Gauges => 1,
        Tab::Mfd => 2,
        Tab::Dtc => 3,
        Tab::Modules => 4,
        Tab::Log => 5,
        Tab::Help => 6,
    };
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(" obdtui "))
        .select(idx)
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, area);
}

fn draw_live(f: &mut Frame, area: Rect, app: &App) {
    let vin = app.session.vin.as_deref().unwrap_or("(not read)");
    let header = format!(
        "VIN: {vin}  ·  bus: {}  ·  poll: {}  ·  capture: {}  ·  PIDs: {}",
        app.session.active_bus(),
        if app.live_poll { "on" } else { "off" },
        if app.capturing {
            if app.session.full_capture {
                "FULL"
            } else {
                "REC"
            }
        } else {
            "off"
        },
        app.session.supported_pids.len()
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(area);

    let info =
        Paragraph::new(header).block(Block::default().borders(Borders::ALL).title(" Session "));
    f.render_widget(info, chunks[0]);

    let rows: Vec<Row> = app
        .live
        .iter()
        .map(|v| {
            Row::new(vec![
                Cell::from(v.name.clone()),
                Cell::from(format!("{:.2}", v.value)),
                Cell::from(v.unit.clone()),
                Cell::from(format!("{:02X}", v.pid)),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(40),
            Constraint::Percentage(25),
            Constraint::Percentage(15),
            Constraint::Percentage(20),
        ],
    )
    .header(
        Row::new(vec!["Signal", "Value", "Unit", "PID"]).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Live data (full Mode 01 when capturing) "),
    );

    f.render_widget(table, chunks[1]);
}

fn draw_gauges(f: &mut Frame, area: Rect, app: &App) {
    let map = app.signal_map();
    let gauges = dashboard_gauges(&map);
    if gauges.is_empty() {
        f.render_widget(
            Paragraph::new("No live signals yet. Wait for poll.")
                .block(Block::default().borders(Borders::ALL).title(" Gauges ")),
            area,
        );
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);

    // Single large gauge
    let idx = app.gauge_index % gauges.len();
    let single = gauges[idx].clone();
    let single_block = Block::default().borders(Borders::ALL).title(format!(
        " Vector single ({}/{})  [n=next] ",
        idx + 1,
        gauges.len()
    ));
    let inner = single_block.inner(chunks[0]);
    f.render_widget(single_block, chunks[0]);
    f.render_widget(single, inner);

    // Row of mini gauges
    let n = gauges.len().clamp(1, 4);
    if chunks[1].width < 8 || chunks[1].height < 4 {
        return;
    }
    let row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Ratio(1, n as u32); n])
        .split(chunks[1]);
    for (i, g) in gauges.into_iter().take(n).enumerate() {
        if i >= row.len() {
            break;
        }
        let b = Block::default().borders(Borders::ALL);
        let inn = b.inner(row[i]);
        f.render_widget(b, row[i]);
        if inn.width >= 6 && inn.height >= 3 {
            f.render_widget(g, inn);
        }
    }
}

fn draw_mfd(f: &mut Frame, area: Rect, app: &App) {
    let map = app.signal_map();
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" MFD · vector HUD (live priority PIDs) ");
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    // Guard tiny terminals — no nested splits that can panic/zero-size.
    if inner.width < 24 || inner.height < 10 {
        f.render_widget(
            Paragraph::new("MFD needs a larger terminal (min ~24x10).")
                .style(Style::default().fg(Color::Yellow)),
            inner,
        );
        return;
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ])
        .split(inner);

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(cols[0]);
    let center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(cols[1]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(cols[2]);

    let get = |k: &str| map.get(k).copied().unwrap_or(0.0);

    f.render_widget(
        ArcGauge::new("RPM", get("engine_rpm"), 0.0, 7000.0, "rpm"),
        left[0],
    );
    f.render_widget(
        ArcGauge {
            label: "LOAD".into(),
            value: get("engine_load"),
            min: 0.0,
            max: 100.0,
            unit: "%".into(),
            accent: Color::Yellow,
        },
        left[1],
    );
    f.render_widget(
        ArcGauge {
            label: "SPD".into(),
            value: get("vehicle_speed"),
            min: 0.0,
            max: 200.0,
            unit: "km/h".into(),
            accent: Color::Green,
        },
        center[0],
    );
    f.render_widget(
        TapeGauge {
            label: "THR".into(),
            value: get("throttle"),
            min: 0.0,
            max: 100.0,
            unit: "%".into(),
        },
        center[1],
    );
    let strip = format!(
        "VIN {}  BUS {}  CAP {}  SIG {}  poll LIVE",
        app.session.vin.as_deref().unwrap_or("-"),
        app.session.active_bus(),
        if app.capturing { "FULL" } else { "off" },
        app.live.len()
    );
    f.render_widget(
        Paragraph::new(strip).style(Style::default().fg(Color::DarkGray)),
        center[2],
    );
    f.render_widget(
        ArcGauge {
            label: "COOL".into(),
            value: get("coolant_temp"),
            min: 40.0,
            max: 120.0,
            unit: "C".into(),
            accent: Color::Red,
        },
        right[0],
    );
    f.render_widget(
        ArcGauge {
            label: "IAT".into(),
            value: get("intake_temp"),
            min: -20.0,
            max: 80.0,
            unit: "C".into(),
            accent: Color::Blue,
        },
        right[1],
    );
}

fn draw_dtc(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    let rows: Vec<Row> = if app.dtcs.is_empty() {
        vec![Row::new(vec![
            Cell::from("(none)"),
            Cell::from("Press r to read DTCs"),
            Cell::from(""),
        ])]
    } else {
        app.dtcs
            .iter()
            .map(|d| {
                Row::new(vec![
                    Cell::from(d.code.clone()),
                    Cell::from(format!("{:?}", d.status)),
                    Cell::from(d.raw.clone()),
                ])
            })
            .collect()
    };

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(30),
            Constraint::Percentage(30),
            Constraint::Percentage(40),
        ],
    )
    .header(
        Row::new(vec!["Code", "Status", "Raw"]).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Diagnostic trouble codes "),
    );
    f.render_widget(table, chunks[0]);

    let ff = if app.freeze_lines.is_empty() {
        "Freeze frame: press r to load.".to_string()
    } else {
        app.freeze_lines.join("\n")
    };
    f.render_widget(
        Paragraph::new(ff)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Freeze frame (Mode 02) "),
            )
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn draw_modules(f: &mut Frame, area: Rect, app: &App) {
    let text = if app.module_lines.is_empty() {
        "Press m to run Ford module read probes (read-only).\n\
Bus b cycles HS/MS protocol select on STN/MX+."
            .to_string()
    } else {
        app.module_lines.join("\n")
    };
    f.render_widget(
        Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Modules (F4 read scaffold) "),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_log(f: &mut Frame, area: Rect, app: &App) {
    let start = app.log.len().saturating_sub(area.height as usize - 2);
    let text: Vec<Line> = app.log[start..]
        .iter()
        .map(|l| Line::from(l.as_str()))
        .collect();
    let p = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(" Log "));
    f.render_widget(p, area);
}

fn draw_help(f: &mut Frame, area: Rect, app: &App) {
    let writes = if app.session.allow_writes {
        "enabled"
    } else {
        "disabled (default)"
    };
    let lines = vec![
        Line::from("Keys"),
        Line::from("  q / Esc  Quit"),
        Line::from("  Tab      Next view"),
        Line::from("  1-7      Live / Gauges / MFD / DTC / Modules / Log / Help"),
        Line::from("  p        Toggle live poll"),
        Line::from("  r        Read DTCs + freeze frame"),
        Line::from("  c        Start/stop FULL Mode 01 capture"),
        Line::from("  n        Next single gauge"),
        Line::from("  m        Module read probes"),
        Line::from("  b        Cycle HS/MS bus (STN path)"),
        Line::from("  x        Clear DTCs (needs --allow-writes)"),
        Line::from(""),
        Line::from(format!("Writes: {writes}")),
        Line::from(format!("Adapter: {}", app.status)),
        Line::from(""),
        Line::from("F1 full capture · F2 vector gauges · F3 freeze frame"),
        Line::from("F4 module probes · F5 gated writes · F6 MFD shell"),
        Line::from("Default mode is read-only. No module programming."),
    ];
    let p = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Help "))
        .wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let err = app
        .last_error
        .as_deref()
        .map(|e| format!(" | {e}"))
        .unwrap_or_default();
    let cap = if app.capturing { "FULL" } else { "off" };
    let text = Line::from(vec![
        Span::styled(
            " q quit ",
            Style::default().fg(Color::Black).bg(Color::Cyan),
        ),
        Span::raw(format!(
            " {} · capture {} · live {}{}",
            app.status,
            cap,
            app.live.len(),
            err
        )),
    ]);
    let p = Paragraph::new(text).block(Block::default().borders(Borders::ALL));
    f.render_widget(p, area);
}
