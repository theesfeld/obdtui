//! Ratatui views.

use crate::app::{App, Tab};
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
        Tab::Dtc => draw_dtc(f, chunks[1], app),
        Tab::Log => draw_log(f, chunks[1], app),
        Tab::Help => draw_help(f, chunks[1], app),
    }
    draw_footer(f, chunks[2], app);
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let titles = ["1 Live", "2 DTC", "3 Log", "4 Help"]
        .iter()
        .cloned()
        .map(Line::from)
        .collect::<Vec<_>>();
    let idx = match app.tab {
        Tab::Live => 0,
        Tab::Dtc => 1,
        Tab::Log => 2,
        Tab::Help => 3,
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
        "VIN: {vin}  ·  bus: {}  ·  poll: {}  ·  capture: {}",
        app.session.active_bus(),
        if app.live_poll { "on" } else { "off" },
        if app.capturing { "REC" } else { "off" },
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
            .title(" Live data (Mode 01) "),
    );

    f.render_widget(table, chunks[1]);
}

fn draw_dtc(f: &mut Frame, area: Rect, app: &App) {
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
    f.render_widget(table, area);
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
        Line::from("  1-4      Live / DTC / Log / Help"),
        Line::from("  p        Toggle live poll"),
        Line::from("  r        Read DTCs"),
        Line::from("  c        Start or stop capture"),
        Line::from("  b        Cycle bus tag (hs / ms / unknown)"),
        Line::from("  x        Clear DTCs (needs --allow-writes)"),
        Line::from(""),
        Line::from(format!("Writes: {writes}")),
        Line::from(format!("Adapter: {}", app.status)),
        Line::from(""),
        Line::from("This is a 0.x tool. Interfaces may change."),
        Line::from("This tool is not a full FORScan replacement."),
        Line::from("Default mode is read-only."),
        Line::from("Links: USB serial and Bluetooth SPP (RFCOMM)."),
        Line::from("  USB: obdtui --port /dev/ttyUSB0"),
        Line::from("  BT:  obdtui --bt-mac AA:BB:CC:DD:EE:FF"),
        Line::from("  or:  obdtui --port /dev/rfcomm0 --prefer bluetooth"),
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
    let text = Line::from(vec![
        Span::styled(
            " q quit ",
            Style::default().fg(Color::Black).bg(Color::Cyan),
        ),
        Span::raw(format!(
            " {} · capture {}{}",
            app.status,
            if app.capturing { "ON" } else { "off" },
            err
        )),
    ]);
    let p = Paragraph::new(text).block(Block::default().borders(Borders::ALL));
    f.render_widget(p, area);
}
