//! Remote execution command
//!
//! Execute commands in running containers via WebSocket.
//! Supports interactive TTY sessions and non-interactive commands.

use std::sync::Arc;

use clap::Args;
use colored::Colorize;
use futures_util::SinkExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt, stdin, stdout};
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{debug, error, info};

use crate::CliConfig;

/// Exec arguments
#[derive(Args)]
pub struct ExecArgs {
    /// App ID to connect to
    app_id: uuid::Uuid,
    
    /// Instance ID (for multi-instance apps)
    #[arg(short, long)]
    instance: Option<uuid::Uuid>,
    
    /// Command to execute
    #[arg(default_value = "/bin/sh")]
    command: String,
    
    /// Allocate a pseudo-TTY
    #[arg(short = 't', long)]
    tty: bool,
    
    /// Keep STDIN open
    #[arg(short = 'i', long)]
    interactive: bool,
    
    /// Environment variables (KEY=VALUE)
    #[arg(short = 'e', long)]
    env: Vec<String>,
    
    /// Working directory
    #[arg(short = 'w', long)]
    workdir: Option<String>,
    
    /// User to run as (username or UID)
    #[arg(short = 'u', long)]
    user: Option<String>,
}

/// Handle exec command
pub async fn handle(args: ExecArgs, config: &CliConfig) -> anyhow::Result<()> {
    let token = config.token.clone()
        .ok_or_else(|| anyhow::anyhow!("Not authenticated. Run `shellwego auth login`"))?;
    
    // Determine if we need an interactive session
    let interactive = args.tty || args.interactive || args.command == "/bin/sh" || args.command == "/bin/bash";
    
    println!(
        "{} Connecting to app {}...",
        "›".blue(),
        args.app_id
    );
    
    if interactive {
        println!("{} Starting interactive session...", "›".blue());
    }
    
    // Build WebSocket URL
    let ws_url = format!(
        "wss://{}/api/v1/apps/{}/exec?token={}",
        config.api_url.trim_start_matches("https://")
            .trim_start_matches("http://"),
        args.app_id,
        token
    );
    
    debug!("Connecting to: {}", ws_url.split('?').next().unwrap_or(&ws_url));
    
    // Connect to exec endpoint
    let (ws_stream, _) = connect_async(&ws_url).await
        .map_err(|e| anyhow::anyhow!("Failed to connect: {}", e))?;
    
    println!("{}", "✓ Connected".green());
    println!();
    
    // Send initial exec request
    let exec_request = ExecRequest {
        command: args.command.clone(),
        instance_id: args.instance,
        tty: args.tty,
        stdin: args.interactive || args.tty,
        env: parse_env_vars(&args.env),
        workdir: args.workdir.clone(),
        user: args.user.clone(),
    };
    
    let (ws_sink, ws_stream) = ws_stream.split();
    let ws_sink = Arc::new(Mutex::new(ws_sink));
    
    // Send exec request
    {
        let mut sink = ws_sink.lock().await;
        let request_json = serde_json::to_string(&exec_request)?;
        sink.send(Message::Text(request_json)).await?;
    }
    
    info!("Exec session started for app {}", args.app_id);
    
    if interactive {
        // Interactive mode: bidirectional communication
        run_interactive_session(ws_sink, ws_stream, args.tty).await?;
    } else {
        // Non-interactive mode: send command and capture output
        run_non_interactive_session(ws_sink, ws_stream).await?;
    }
    
    println!();
    println!("{} Session ended", "›".blue());
    
    Ok(())
}

/// Run interactive session with TTY
async fn run_interactive_session(
    ws_sink: Arc<Mutex<futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, Message>>>,
    mut ws_stream: futures_util::stream::SplitStream<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>,
    tty: bool,
) -> anyhow::Result<()> {
    use crossterm::{
        event::{self, Event, KeyCode, KeyModifiers},
        execute, terminal,
    };
    
    // Setup terminal for raw mode if TTY
    if tty {
        terminal::enable_raw_mode()?;
        let _guard = RawModeGuard;
    }
    
    let mut stdin = stdin();
    let mut stdout = stdout();
    
    let mut input_buffer = Vec::new();
    
    loop {
        tokio::select! {
            // Read from stdin, send to WebSocket
            result = stdin.read_u8() => {
                match result {
                    Ok(byte) => {
                        // Handle special keys
                        if byte == 3 { // Ctrl+C
                            let mut sink = ws_sink.lock().await;
                            sink.send(Message::Binary(vec![3])).await?;
                            break;
                        }
                        
                        input_buffer.push(byte);
                        
                        // Send to WebSocket
                        let mut sink = ws_sink.lock().await;
                        sink.send(Message::Binary(vec![byte])).await?;
                    }
                    Err(e) => {
                        debug!("Stdin error: {}", e);
                        break;
                    }
                }
            }
            
            // Read from WebSocket, write to stdout
            result = ws_stream.next() => {
                match result {
                    Some(Ok(Message::Binary(data))) => {
                        stdout.write_all(&data).await?;
                        stdout.flush().await?;
                    }
                    Some(Ok(Message::Text(text))) => {
                        // Handle JSON messages
                        if let Ok(msg) = serde_json::from_str::<ExecMessage>(&text) {
                            match msg {
                                ExecMessage::Output { data } => {
                                    stdout.write_all(data.as_bytes()).await?;
                                    stdout.flush().await?;
                                }
                                ExecMessage::Error { message } => {
                                    eprintln!("{} {}", "Error:".red(), message);
                                }
                                ExecMessage::Exit { code } => {
                                    debug!("Process exited with code {}", code);
                                    break;
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        debug!("WebSocket closed");
                        break;
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        debug!("WebSocket stream ended");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
    
    Ok(())
}

/// Run non-interactive session
async fn run_non_interactive_session(
    ws_sink: Arc<Mutex<futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, Message>>>,
    mut ws_stream: futures_util::stream::SplitStream<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>,
) -> anyhow::Result<()> {
    let mut stdout = stdout();
    
    // Send EOF to signal no more input
    {
        let mut sink = ws_sink.lock().await;
        sink.send(Message::Binary(vec![])).await?;
    }
    
    // Read output until stream ends
    while let Some(result) = ws_stream.next().await {
        match result {
            Ok(Message::Binary(data)) => {
                stdout.write_all(&data).await?;
                stdout.flush().await?;
            }
            Ok(Message::Text(text)) => {
                if let Ok(msg) = serde_json::from_str::<ExecMessage>(&text) {
                    match msg {
                        ExecMessage::Output { data } => {
                            stdout.write_all(data.as_bytes()).await?;
                            stdout.flush().await?;
                        }
                        ExecMessage::Error { message } => {
                            eprintln!("{} {}", "Error:".red(), message);
                        }
                        ExecMessage::Exit { code } => {
                            if code != 0 {
                                return Err(anyhow::anyhow!("Command exited with code {}", code));
                            }
                            break;
                        }
                    }
                }
            }
            Ok(Message::Close(_)) => break,
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }
    
    Ok(())
}

/// Parse environment variables from KEY=VALUE format
fn parse_env_vars(envs: &[String]) -> HashMap<String, String> {
    let mut result = HashMap::new();
    
    for env in envs {
        if let Some((key, value)) = env.split_once('=') {
            result.insert(key.to_string(), value.to_string());
        } else {
            // No value - will use empty string
            result.insert(env.clone(), String::new());
        }
    }
    
    result
}

/// Guard to restore terminal mode on drop
struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

// --- Data Structures ---

/// Exec request message
#[derive(Debug, Serialize)]
pub struct ExecRequest {
    /// Command to execute
    pub command: String,
    /// Instance ID
    pub instance_id: Option<uuid::Uuid>,
    /// Allocate TTY
    pub tty: bool,
    /// Attach stdin
    pub stdin: bool,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Working directory
    pub workdir: Option<String>,
    /// User to run as
    pub user: Option<String>,
}

/// Exec message types
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ExecMessage {
    /// Output data
    #[serde(rename = "output")]
    Output { data: String },
    
    /// Error message
    #[serde(rename = "error")]
    Error { message: String },
    
    /// Process exit
    #[serde(rename = "exit")]
    Exit { code: i32 },
}

/// Resize TTY message
#[derive(Debug, Serialize)]
pub struct ResizeMessage {
    /// Terminal width
    pub cols: u16,
    /// Terminal height
    pub rows: u16,
}

/// Signal message (for sending signals to process)
#[derive(Debug, Serialize)]
pub struct SignalMessage {
    /// Signal name (e.g., "SIGTERM", "SIGKILL")
    pub signal: String,
}

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
