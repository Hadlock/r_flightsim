use std::io;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Modifier},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};

#[derive(Clone)]
pub struct RadioLogEntry {
    pub frequency: f32,
    pub speaker: String,
    pub text: String,
}

#[derive(Clone)]
pub struct Telemetry {
    pub airspeed_kts: f64,
    pub groundspeed_kts: f64,
    pub vertical_speed_fpm: f64,
    pub altitude_msl_ft: f64,
    pub altitude_agl_ft: f64,
    pub heading_deg: f64,
    pub pitch_deg: f64,
    pub bank_deg: f64,
    pub throttle_pct: f64,
    pub alpha_deg: f64,
    pub on_ground: bool,
    pub brakes: bool,
    pub latitude: f64,
    pub longitude: f64,
    pub fps: f64,
    pub aircraft_name: String,
    pub app_state: AppStateLabel,
    pub radio_log: Vec<RadioLogEntry>,
}

#[derive(Clone, PartialEq)]
pub enum AppStateLabel {
    Menu,
    Flying,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self {
            airspeed_kts: 0.0,
            groundspeed_kts: 0.0,
            vertical_speed_fpm: 0.0,
            altitude_msl_ft: 0.0,
            altitude_agl_ft: 0.0,
            heading_deg: 0.0,
            pitch_deg: 0.0,
            bank_deg: 0.0,
            throttle_pct: 0.0,
            alpha_deg: 0.0,
            on_ground: true,
            brakes: false,
            latitude: 0.0,
            longitude: 0.0,
            fps: 0.0,
            aircraft_name: String::new(),
            app_state: AppStateLabel::Menu,
            radio_log: Vec::new(),
        }
    }
}

pub type SharedTelemetry = Arc<Mutex<Telemetry>>;

pub fn new_shared_telemetry() -> SharedTelemetry {
    Arc::new(Mutex::new(Telemetry::default()))
}

/// Spawn the ratatui dashboard thread. Returns the join handle.
/// The thread runs until `shutdown` is set to true.
pub fn spawn_dashboard(
    telemetry: SharedTelemetry,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        if let Err(e) = run_dashboard(telemetry, shutdown) {
            eprintln!("Dashboard error: {}", e);
        }
    })
}

fn run_dashboard(
    telemetry: SharedTelemetry,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
) -> io::Result<()> {
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    loop {
        if shutdown.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }

        // Poll for terminal events (non-blocking, 100ms timeout for ~10Hz)
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    // Don't quit on q - that's for the main app
                }
            }
        }

        let telem = telemetry.lock().unwrap().clone();

        terminal.draw(|frame| {
            let area = frame.area();

            if telem.app_state == AppStateLabel::Menu {
                draw_menu_screen(frame, area);
            } else {
                draw_flight_dashboard(frame, area, &telem);
            }
        })?;
    }

    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn draw_menu_screen(frame: &mut ratatui::Frame, area: Rect) {
    let block = Block::default()
        .title(" shaderflight ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let text = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "  shaderflight — menu active",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Select an aircraft and click FLY NOW",
            Style::default().fg(Color::Gray),
        )),
    ])
    .block(block);

    frame.render_widget(text, area);
}

fn draw_flight_dashboard(frame: &mut ratatui::Frame, area: Rect, t: &Telemetry) {
    let outer_block = Block::default()
        .title(format!(" shaderflight — {} ", t.aircraft_name))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    // Split into rows
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Attitude
            Constraint::Length(3), // Speeds
            Constraint::Length(3), // Altitude
            Constraint::Length(3), // Engine
            Constraint::Length(3), // Position
            Constraint::Min(5),   // Radio log
        ])
        .split(inner);

    // Attitude
    let wow = if t.on_ground { "GND" } else { "AIR" };
    let brk = if t.brakes { "BRK" } else { "   " };
    let attitude = Paragraph::new(Line::from(vec![
        Span::styled(" HDG ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:06.1}°", t.heading_deg), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(" PIT ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:+5.1}°", t.pitch_deg), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(" BNK ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:+5.1}°", t.bank_deg), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(format!(" {} {} ", wow, brk), Style::default().fg(Color::Yellow)),
    ]))
    .block(Block::default().title(" Attitude ").borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));
    frame.render_widget(attitude, chunks[0]);

    // Speeds
    let speeds = Paragraph::new(Line::from(vec![
        Span::styled(" IAS ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:5.0} kt", t.airspeed_kts), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(" GS ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:5.0} kt", t.groundspeed_kts), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(" VS ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:+6.0} fpm", t.vertical_speed_fpm), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]))
    .block(Block::default().title(" Speed ").borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));
    frame.render_widget(speeds, chunks[1]);

    // Altitude
    let altitude = Paragraph::new(Line::from(vec![
        Span::styled(" MSL ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:7.0} ft", t.altitude_msl_ft), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(" AGL ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:7.0} ft", t.altitude_agl_ft), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]))
    .block(Block::default().title(" Altitude ").borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));
    frame.render_widget(altitude, chunks[2]);

    // Engine
    let engine = Paragraph::new(Line::from(vec![
        Span::styled(" THR ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:5.1}%", t.throttle_pct), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(" AoA ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:+5.1}°", t.alpha_deg), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(" FPS ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:5.1}", t.fps), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]))
    .block(Block::default().title(" Engine ").borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));
    frame.render_widget(engine, chunks[3]);

    // Position
    let lat_str = format!("{:.4}°{}", t.latitude.abs(), if t.latitude >= 0.0 { "N" } else { "S" });
    let lon_str = format!("{:.4}°{}", t.longitude.abs(), if t.longitude >= 0.0 { "E" } else { "W" });
    let position = Paragraph::new(Line::from(vec![
        Span::styled(" LAT ", Style::default().fg(Color::DarkGray)),
        Span::styled(lat_str, Style::default().fg(Color::White)),
        Span::raw("  "),
        Span::styled(" LON ", Style::default().fg(Color::DarkGray)),
        Span::styled(lon_str, Style::default().fg(Color::White)),
    ]))
    .block(Block::default().title(" Position ").borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));
    frame.render_widget(position, chunks[4]);

    // Radio log
    let radio_lines: Vec<Line> = t.radio_log.iter().rev().take(20).rev().map(|entry| {
        Line::from(vec![
            Span::styled(format!("{:5.1} ", entry.frequency), Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:<8} ", entry.speaker), Style::default().fg(Color::Cyan)),
            Span::styled(&entry.text, Style::default().fg(Color::White)),
        ])
    }).collect();
    let radio = Paragraph::new(radio_lines)
        .block(Block::default().title(" Radio ").borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));
    frame.render_widget(radio, chunks[5]);
}
