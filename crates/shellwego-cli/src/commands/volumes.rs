//! Volume management commands

use clap::{Args, Subcommand};

use crate::{CliConfig, OutputFormat};

#[derive(Args)]
pub struct VolumeArgs {
    #[command(subcommand)]
    command: VolumeCommands,
}

#[derive(Subcommand)]
enum VolumeCommands {
    List,
    Create { name: String, size_gb: u64 },
    Get { id: uuid::Uuid },
    Delete { id: uuid::Uuid },
    Attach { id: uuid::Uuid, app_id: uuid::Uuid },
    Detach { id: uuid::Uuid },
    Snapshot { id: uuid::Uuid, name: String },
}

pub async fn handle(args: VolumeArgs, config: &CliConfig, format: OutputFormat) -> anyhow::Result<()> {
    let client = crate::client(config)?;

    match args.command {
        VolumeCommands::List => {
            let resp = client.get("/v1/volumes").send().await?;
            let volumes: serde_json::Value = resp.json().await?;
            print_response(&volumes, &format);
        }
        VolumeCommands::Create { name, size_gb } => {
            let body = serde_json::json!({
                "name": name,
                "size_gb": size_gb,
                "volume_type": "persistent",
                "filesystem": "ext4",
                "encrypted": false
            });
            let resp = client.post("/v1/volumes").json(&body).send().await?;
            let volume: serde_json::Value = resp.json().await?;
            print_response(&volume, &format);
        }
        VolumeCommands::Get { id } => {
            let resp = client.get(&format!("/v1/volumes/{}", id)).send().await?;
            let volume: serde_json::Value = resp.json().await?;
            print_response(&volume, &format);
        }
        VolumeCommands::Delete { id } => {
            client.delete(&format!("/v1/volumes/{}", id)).send().await?;
            println!("Volume {} deleted", id);
        }
        VolumeCommands::Attach { id, app_id } => {
            let body = serde_json::json!({ "app_id": app_id });
            let resp = client.post(&format!("/v1/volumes/{}/attach", id)).json(&body).send().await?;
            let result: serde_json::Value = resp.json().await?;
            print_response(&result, &format);
        }
        VolumeCommands::Detach { id } => {
            let resp = client.post(&format!("/v1/volumes/{}/detach", id)).send().await?;
            let result: serde_json::Value = resp.json().await?;
            print_response(&result, &format);
        }
        VolumeCommands::Snapshot { id, name } => {
            let body = serde_json::json!({ "name": name });
            let resp = client.post(&format!("/v1/volumes/{}/snapshots", id)).json(&body).send().await?;
            let snapshot: serde_json::Value = resp.json().await?;
            print_response(&snapshot, &format);
        }
    }

    Ok(())
}

fn print_response(value: &serde_json::Value, format: &OutputFormat) {
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(value).unwrap()),
        OutputFormat::Text => println!("{}", value),
    }
}
