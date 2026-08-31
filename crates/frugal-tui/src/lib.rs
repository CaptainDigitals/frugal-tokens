//! Frugal TUI — interactive terminal dashboard (Ratatui + Crossterm).
//!
//! Keys: ←/→ or Tab switch screens · r refresh · c checkpoint list ·
//! p providers · ? help · q quit. Accessibility: keyboard-only, text labels
//! next to every color signal, degrades to any terminal ≥80 columns.

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Cell, Gauge, List, ListItem, Paragraph, Row, Table, Tabs},
};
use std::time::{Duration, Instant};

const TABS: [&str; 6] = [
    "Overview",
    "Tools",
    "Budget",
    "Providers",
    "Checkpoints",
    "Audit",
];
const NAVY: Color = Color::Rgb(16, 28, 48);
const CYAN: Color = Color::Rgb(64, 200, 210);
const GOLD: Color = Color::Rgb(228, 178, 84);

struct App {
    tab: usize,
    session: Option<frugal_storage::SessionRow>,
    today: Option<frugal_core::TodayStats>,
    tools: Vec<frugal_storage::ToolRow>,
    sessions: Vec<frugal_storage::SessionRow>,
    events: Vec<frugal_storage::EventRow>,
    checkpoints: Vec<frugal_storage::CheckpointRow>,
    providers: Vec<(String, String, bool, i64)>,
    health: i64,
    profile: String,
    budgets: frugal_policy::Budgets,
    show_help: bool,
    error: Option<String>,
}

impl App {
    fn new() -> Self {
        let mut app = App {
            tab: 0,
            session: None,
            today: None,
            tools: vec![],
            sessions: vec![],
            events: vec![],
            checkpoints: vec![],
            providers: vec![],
            health: 100,
            profile: "shadow".into(),
            budgets: frugal_policy::Budgets::default(),
            show_help: false,
            error: None,
        };
        app.refresh();
        app
    }

    fn refresh(&mut self) {
        self.error = None;
        match self.try_refresh() {
            Ok(()) => {}
            Err(err) => self.error = Some(err.to_string()),
        }
    }

    fn try_refresh(&mut self) -> Result<()> {
        let conn = frugal_storage::open()?;
        self.session = frugal_storage::latest_session(&conn)?;
        self.today = Some(frugal_core::today_stats(&conn)?);
        self.tools = frugal_storage::tools_today(&conn)?;
        self.sessions = frugal_storage::recent_sessions(&conn, 8)?;
        self.events = frugal_storage::recent_events(&conn, 12)?;
        self.checkpoints = frugal_storage::list_checkpoints(&conn, 12)?;
        self.health = frugal_core::health_score(&conn)?;
        let cfg = frugal_policy::load();
        self.profile = cfg.profile;
        self.budgets = cfg.budgets;
        self.providers = frugal_providers::registry()
            .iter()
            .map(|p| {
                (
                    p.id.clone(),
                    p.trust.clone(),
                    frugal_providers::installed(p),
                    p.fixed_context_tax_tokens,
                )
            })
            .collect();
        Ok(())
    }
}

pub fn run() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = event_loop(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn event_loop(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
    let mut app = App::new();
    let mut last_refresh = Instant::now();
    loop {
        terminal.draw(|frame| draw(frame, &app))?;
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Right | KeyCode::Tab => app.tab = (app.tab + 1) % TABS.len(),
                    KeyCode::Left => app.tab = (app.tab + TABS.len() - 1) % TABS.len(),
                    KeyCode::Char('r') => app.refresh(),
                    KeyCode::Char('p') => app.tab = 3,
                    KeyCode::Char('b') => app.tab = 2,
                    KeyCode::Char('c') => app.tab = 4,
                    KeyCode::Char('?') => app.show_help = !app.show_help,
                    _ => {}
                }
            }
        }
        if last_refresh.elapsed() >= Duration::from_secs(2) {
            app.refresh();
            last_refresh = Instant::now();
        }
    }
}

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(4),
            Constraint::Length(1),
        ])
        .split(area);

    let title = Line::from(vec![
        Span::styled("◈ Frugal Tokenomics ", Style::default().fg(GOLD).bold()),
        Span::styled("Community Edition", Style::default().fg(CYAN)),
        Span::raw("  "),
        Span::styled(
            format!("LIVE · profile {} · health {}/100", app.profile, app.health),
            Style::default().fg(Color::Gray),
        ),
    ]);
    frame.render_widget(Paragraph::new(title).bg(NAVY), chunks[0]);

    let tabs = Tabs::new(TABS.iter().map(|t| Line::from(*t)).collect::<Vec<_>>())
        .select(app.tab)
        .highlight_style(Style::default().fg(GOLD).bold())
        .divider("│");
    frame.render_widget(tabs, chunks[1]);

    if app.show_help {
        draw_help(frame, chunks[2]);
    } else {
        match app.tab {
            0 => draw_overview(frame, chunks[2], app),
            1 => draw_tools(frame, chunks[2], app),
            2 => draw_budget(frame, chunks[2], app),
            3 => draw_providers(frame, chunks[2], app),
            4 => draw_checkpoints(frame, chunks[2], app),
            _ => draw_audit(frame, chunks[2], app),
        }
    }

    let footer = match &app.error {
        Some(err) => Line::from(Span::styled(
            format!("runtime error (fail-open): {err}"),
            Style::default().fg(Color::Red),
        )),
        None => Line::from(Span::styled(
            " ←/→ screens · r refresh · b budget · p providers · c checkpoints · ? help · q quit",
            Style::default().fg(Color::DarkGray),
        )),
    };
    frame.render_widget(Paragraph::new(footer), chunks[3]);
}

fn block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(CYAN).bold(),
        ))
}

fn draw_overview(frame: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(4)])
        .split(area);
    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[0]);

    let ctx = app
        .session
        .as_ref()
        .and_then(|s| s.context_pct)
        .unwrap_or(0.0);
    let ctx_label = pressure_label(ctx);
    let gauge = Gauge::default()
        .block(block("CONTEXT PRESSURE"))
        .gauge_style(Style::default().fg(pressure_color(ctx)))
        .ratio((ctx / 100.0).clamp(0.0, 1.0))
        .label(format!("{ctx:.0}% {ctx_label}"));
    frame.render_widget(gauge, top[0]);

    let today = app.today.as_ref();
    let cost = app.session.as_ref().and_then(|s| s.cost_usd);
    let lines = vec![
        Line::from(format!(
            "session cost   {}",
            cost.map(|c| format!("${c:.2}"))
                .unwrap_or_else(|| "-".into())
        )),
        Line::from(format!(
            "today spend    ${:.2} across {} session(s)",
            today.map(|t| t.spend_usd).unwrap_or(0.0),
            today.map(|t| t.sessions).unwrap_or(0)
        )),
        Line::from(format!(
            "duplicate calls today: {}  (~{} wasted tokens)",
            today.map(|t| t.duplicate_calls).unwrap_or(0),
            today.map(|t| t.duplicate_tokens).unwrap_or(0)
        )),
    ];
    frame.render_widget(Paragraph::new(lines).block(block("ECONOMICS")), top[1]);

    let session_rows: Vec<Row> = app
        .sessions
        .iter()
        .map(|s| {
            Row::new(vec![
                Cell::from(s.id.chars().take(14).collect::<String>()),
                Cell::from(s.model.clone().unwrap_or_else(|| "-".into())),
                Cell::from(
                    s.cost_usd
                        .map(|c| format!("${c:.2}"))
                        .unwrap_or_else(|| "-".into()),
                ),
                Cell::from(
                    s.context_pct
                        .map(|c| format!("{c:.0}%"))
                        .unwrap_or_else(|| "-".into()),
                ),
                Cell::from(format!("+{}/-{}", s.lines_added, s.lines_removed)),
            ])
        })
        .collect();
    let table = Table::new(
        session_rows,
        [
            Constraint::Length(16),
            Constraint::Length(18),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Length(12),
        ],
    )
    .header(
        Row::new(vec!["session", "model", "cost", "ctx", "lines"]).style(Style::default().fg(GOLD)),
    )
    .block(block("RECENT SESSIONS"));
    frame.render_widget(table, rows[1]);
}

fn draw_tools(frame: &mut Frame, area: Rect, app: &App) {
    let rows: Vec<Row> = app
        .tools
        .iter()
        .map(|t| {
            let style = if t.duplicates > 0 {
                Style::default().fg(GOLD)
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(t.tool.clone()),
                Cell::from(t.calls.to_string()),
                Cell::from(t.duplicates.to_string()),
                Cell::from(format!("{}", t.est_tokens)),
            ])
            .style(style)
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(24),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(14),
        ],
    )
    .header(
        Row::new(vec!["tool", "calls", "duplicate", "est. tokens"])
            .style(Style::default().fg(GOLD)),
    )
    .block(block("TOOL ANALYTICS — TODAY (gold = duplicate waste)"));
    frame.render_widget(table, area);
}

fn draw_budget(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Min(2),
        ])
        .split(area);

    let session_spend = app.session.as_ref().and_then(|s| s.cost_usd).unwrap_or(0.0);
    render_budget_gauge(
        frame,
        chunks[0],
        "SESSION BUDGET",
        session_spend,
        app.budgets.session_usd,
    );
    let today_spend = app.today.as_ref().map(|t| t.spend_usd).unwrap_or(0.0);
    render_budget_gauge(
        frame,
        chunks[1],
        "DAILY BUDGET",
        today_spend,
        app.budgets.daily_usd,
    );
    let note = Paragraph::new(
        "Community Edition warns (! at 80%, X when exceeded) — it never blocks.\n\
         Set budgets: frugal budget set <task|session|daily> <usd>",
    )
    .style(Style::default().fg(Color::Gray));
    frame.render_widget(note, chunks[2]);
}

fn render_budget_gauge(frame: &mut Frame, area: Rect, title: &str, spend: f64, limit: Option<f64>) {
    match limit {
        Some(limit) if limit > 0.0 => {
            let ratio = (spend / limit).clamp(0.0, 1.0);
            let health = frugal_policy::budget_health(Some(spend), Some(limit));
            let color = match health {
                'X' => Color::Red,
                '!' => GOLD,
                _ => Color::Green,
            };
            let gauge = Gauge::default()
                .block(block(title))
                .gauge_style(Style::default().fg(color))
                .ratio(ratio)
                .label(format!("${spend:.2} / ${limit:.2}  [{health}]"));
            frame.render_widget(gauge, area);
        }
        _ => {
            let p = Paragraph::new(format!("${spend:.2} spent — no limit set")).block(block(title));
            frame.render_widget(p, area);
        }
    }
}

fn draw_providers(frame: &mut Frame, area: Rect, app: &App) {
    let rows: Vec<Row> = app
        .providers
        .iter()
        .map(|(id, trust, installed, tax)| {
            Row::new(vec![
                Cell::from(id.clone()),
                Cell::from(trust.clone()),
                Cell::from(if *installed { "INSTALLED" } else { "available" }),
                Cell::from(format!("{tax}")),
            ])
            .style(if *installed {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            })
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(22),
            Constraint::Length(14),
            Constraint::Length(12),
            Constraint::Length(10),
        ],
    )
    .header(
        Row::new(vec!["provider", "trust", "status", "ctx tax"]).style(Style::default().fg(GOLD)),
    )
    .block(block("PROVIDERS"));
    frame.render_widget(table, area);
}

fn draw_checkpoints(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .checkpoints
        .iter()
        .map(|c| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", c.ts), Style::default().fg(Color::Gray)),
                Span::styled(format!("{:<28}", c.note), Style::default().fg(CYAN)),
                Span::raw(c.path.clone()),
            ]))
        })
        .collect();
    let list = List::new(items).block(block("CHECKPOINTS (newest first)"));
    frame.render_widget(list, area);
}

fn draw_audit(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .events
        .iter()
        .map(|e| {
            let color = match e.kind.as_str() {
                "duplicate_tool_call" => GOLD,
                "pre_compact_checkpoint" => CYAN,
                _ => Color::Gray,
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", e.ts), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:<24}", e.kind), Style::default().fg(color)),
                Span::raw(e.detail.clone()),
            ]))
        })
        .collect();
    let list = List::new(items).block(block("AUDIT — RECENT OBSERVATIONS"));
    frame.render_widget(list, area);
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let text = "\
  ← / →  or Tab   switch screens
  r               refresh now (auto-refresh every 2s)
  b               budget screen
  p               providers screen
  c               checkpoints screen
  ?               toggle this help
  q / Esc         quit

  Frugal is local-first: everything on screen comes from ~/.frugal/frugal.db.
  SHADOW profile observes only — no workflow intervention.";
    frame.render_widget(Paragraph::new(text).block(block("HELP")), area);
}

fn pressure_label(pct: f64) -> &'static str {
    match pct {
        p if p >= 90.0 => "CRITICAL",
        p if p >= 75.0 => "RED",
        p if p >= 60.0 => "ORANGE",
        p if p >= 40.0 => "YELLOW",
        _ => "GREEN",
    }
}

fn pressure_color(pct: f64) -> Color {
    match pct {
        p if p >= 75.0 => Color::Red,
        p if p >= 60.0 => Color::Rgb(230, 140, 60),
        p if p >= 40.0 => GOLD,
        _ => Color::Green,
    }
}
