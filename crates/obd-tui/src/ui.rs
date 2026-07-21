//! Ratatui views. HUD tab draws VECTOR lines with Braille canvas (LOGO-style).

use crate::app::{App, Tab};
use crate::term_hud;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Tabs, Wrap};
use ratatui::Frame;

pub fn draw(f: &mut Frame, app: &mut App) {
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
        Tab::Hud => term_hud::draw(f, chunks[1], app),
        Tab::Dtc => draw_dtc(f, chunks[1], app),
        Tab::Modules => draw_modules(f, chunks[1], app),
        Tab::Log => draw_log(f, chunks[1], app),
        Tab::Help => draw_help(f, chunks[1], app),
    }
    draw_footer(f, chunks[2], app);
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let titles = ["1 Live", "2 HUD", "3 DTC", "4 Modules", "5 Log", "6 Help"]
        .iter()
        .cloned()
        .map(Line::from)
        .collect::<Vec<_>>();
    let idx = match app.tab {
        Tab::Live => 0,
        Tab::Hud => 1,
        Tab::Dtc => 2,
        Tab::Modules => 3,
        Tab::Log => 4,
        Tab::Help => 5,
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

fn draw_live(f: &mut Frame, area: Rect, app: &mut App) {
    let vin = app.session.vin.as_deref().unwrap_or("(not read)");
    let sel = app.live_table.selected().map(|i| i + 1).unwrap_or(0);
    let header = format!(
        "VIN: {vin}  ·  bus: {}  ·  poll: {}  ·  capture: {}  ·  PIDs: {}  ·  row {}/{}  (arrows)",
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
        app.session.supported_pids.len(),
        sel,
        app.live.len()
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(area);

    let info =
        Paragraph::new(header).block(Block::default().borders(Borders::ALL).title(" Session "));
    f.render_widget(info, chunks[0]);

    // Header + borders take ~3 rows; keep page size for PgUp/PgDn.
    app.live_page_rows = chunks[1].height.saturating_sub(3).max(1) as usize;

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
    .row_highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol("> ")
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Live data  ·  Up/Down  PgUp/PgDn  Home/End "),
    );

    f.render_stateful_widget(table, chunks[1], &mut app.live_table);
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
        Line::from("  1-6      Live / HUD / DTC / Modules / Log / Help"),
        Line::from("  Up/Down  Scroll Live table (also j/k)"),
        Line::from("  PgUp/Dn  Page Live table · Home/End ends"),
        Line::from("  p        Toggle live poll"),
        Line::from("  r        Read DTCs + freeze frame"),
        Line::from("  c        Start/stop FULL Mode 01 capture"),
        Line::from("  m        Module read probes"),
        Line::from("  b        Cycle HS/MS bus (STN path)"),
        Line::from("  x        Clear DTCs (needs --allow-writes)"),
        Line::from(""),
        Line::from("Tab 2 HUD: VECTOR lines in-terminal (Braille canvas)."),
        Line::from("Optional GPU window: cargo run -p obd-mfd -- --bt-mac <MAC>"),
        Line::from(""),
        Line::from(format!("Writes: {writes}")),
        Line::from(format!("Adapter: {}", app.status)),
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
