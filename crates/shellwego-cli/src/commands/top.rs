//! Top - Real-time resource monitoring TUI
//!
//! Displays a beautiful dashboard showing nodes, apps, and resources
//! with live updates using ratatui.

use crate::{client::ApiClient, config::CliConfig};
use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::time::{Duration, Instant};
use shellwego_core::entities::{app::App, node::Node};

#[derive(clap::Args)]
pub struct TopArgs {
    /// Refresh interval in milliseconds
    #[arg(short, long, default_value = "1000")]
    pub interval: u64,

    /// Focus on specific node
    #[arg(short, long)]
    pub node: Option<String>,

    /// Show only apps
    #[arg(long)]
    pub apps_only: bool,

    /// Show only nodes
    #[arg(long)]
    pub nodes_only: bool,
}

pub async fn handle(args: TopArgs, config: &CliConfig) -> Result<()> {
    let client = crate::client(config)?;
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_loop(&mut terminal, client, args).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    res
}

struct DashboardState {
    apps: Vec<App>,
    nodes: Vec<Node>,
    error: Option<String>,
}

impl DashboardState {
    async fn refresh(&mut self, client: &ApiClient) -> Result<()> {
        let (apps, nodes) = tokio::try_join!(client.list_apps(), client.list_nodes())?;
        self.apps = apps;
        self.nodes = nodes;
        self.error = None;
        Ok(())
    }
}

async fn run_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    client: ApiClient,
    args: TopArgs,
) -> Result<()> {
    let tick_rate = Duration::from_millis(args.interval);
    let mut last_tick = Instant::now();
    let mut state = DashboardState {
        apps: Vec::new(),
        nodes: Vec::new(),
        error: None,
    };

    let _ = state.refresh(&client).await;

    loop {
        terminal.draw(|f| ui::render(f, &state, &args))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if let KeyCode::Char('q') = key.code {
                    return Ok(());
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            if let Err(e) = state.refresh(&client).await {
                state.error = Some(e.to_string());
            }
            last_tick = Instant::now();
        }
    }
}

mod ui {
    use super::*;

    pub fn render(f: &mut Frame, state: &DashboardState, args: &TopArgs) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(1),
            ])
            .split(f.area());

        let status_text = if let Some(err) = &state.error {
            format!(" Error: {}", err).red()
        } else {
            format!(" Nodes: {} | Apps: {}", state.nodes.len(), state.apps.len()).green()
        };
        f.render_widget(Paragraph::new(status_text).block(Block::bordered().title("ShellWeGo Top")), chunks[0]);

        let content_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(if args.apps_only { 0 } else if args.nodes_only { 100 } else { 40 }),
                Constraint::Percentage(if args.nodes_only { 0 } else if args.apps_only { 100 } else { 60 }),
            ])
            .split(chunks[1]);

        if !args.apps_only {
            let rows = state.nodes.iter().map(|n| {
                Row::new(vec![
                    n.hostname.clone(),
                    format!("{:?}", n.status),
                    n.region.clone(),
                    format!("{:.1}vCPU", n.capacity.cpu_available),
                ])
            });
            let table = Table::new(rows, [
                Constraint::Percentage(30),
                Constraint::Percentage(20),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
            ])
            .header(Row::new(vec!["Node", "Status", "Region", "Free"]).style(Style::default().bold()))
            .block(Block::bordered().title(" Nodes "));
            f.render_widget(table, content_chunks[0]);
        }

        if !args.nodes_only {
            let rows = state.apps.iter()
                .filter(|a| args.node.as_ref().map_or(true, |n| n == &a.name || n == &a.id.to_string()))
                .map(|a| {
                    Row::new(vec![
                        a.name.clone(),
                        format!("{:?}", a.status),
                        a.image.chars().take(20).collect(),
                        a.resources.memory.clone(),
                    ])
                });
            let table = Table::new(rows, [
                Constraint::Percentage(30),
                Constraint::Percentage(20),
                Constraint::Percentage(30),
                Constraint::Percentage(20),
            ])
            .header(Row::new(vec!["App", "Status", "Image", "Mem"]).style(Style::default().bold()))
            .block(Block::bordered().title(" Apps "));
            f.render_widget(table, content_chunks[1]);
        }

        let help = " [q] Quit | [r] Refresh | ".dimmed();
        f.render_widget(Paragraph::new(help), chunks[2]);
    }
}
