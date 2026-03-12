//! Interactive shell (REPL) for ShellWeGo

use rustyline::{Editor, error::ReadlineError};

/// Start interactive shell
pub async fn run(config: &crate::CliConfig) -> anyhow::Result<()> {
    println!("ShellWeGo Interactive Shell");
    println!("Type 'help' for commands, 'exit' to quit");
    
    let mut rl = Editor::<()>::new()?;
    
    loop {
        let readline = rl.readline("shellwego> ");
        match readline {
            Ok(line) => {
                rl.add_history_entry(line.as_str());
                
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.is_empty() {
                    continue;
                }
                
                match parts[0] {
                    "exit" | "quit" => break,
                    "help" => print_help(),
                    "apps" => println!("Use: apps list, apps create, etc."),
                    // TODO: Dispatch to command handlers
                    _ => println!("Unknown command: {}", parts[0]),
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }
    
    Ok(())
}

fn print_help() {
    println!("Available commands:");
    println!("  apps        Manage applications");
    println!("  nodes       Manage worker nodes");
    println!("  volumes     Manage persistent volumes");
    println!("  domains     Manage custom domains");
    println!("  status      Show system status");
    println!("  exit        Exit shell");
}