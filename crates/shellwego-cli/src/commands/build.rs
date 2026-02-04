//! Local build and push command

use clap::Args;
use colored::Colorize;

use crate::CliConfig;

#[derive(Args)]
pub struct BuildArgs {
    /// Directory to build
    #[arg(default_value = ".")]
    path: std::path::PathBuf,
    
    /// Tag for the image
    #[arg(short, long)]
    tag: Option<String>,
    
    /// Push after build
    #[arg(short, long)]
    push: bool,
    
    /// Build arguments
    #[arg(short, long)]
    build_arg: Vec<String>,
    
    /// Use buildpack instead of Dockerfile
    #[arg(long)]
    buildpack: bool,
}

pub async fn handle(args: BuildArgs, config: &CliConfig) -> anyhow::Result<()> {
    println!("{} Building from {:?}...", "→".blue(), args.path);
    
    // TODO: Detect Dockerfile or buildpack.toml
    // TODO: Connect to remote builder or use local buildkit
    // TODO: Stream build logs
    // TODO: Tag result
    // TODO: Push if --push
    
    if args.buildpack {
        println!("{}", "Using Cloud Native Buildpacks...".dimmed());
    }
    
    println!("{}", "Build not yet implemented".yellow());
    println!("Use `docker build` and `docker push` for now");
    
    Ok(())
}