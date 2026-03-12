//! Docker Compose import and management
//!
//! Convert docker-compose.yml files to ShellWeGo applications
//! and export ShellWeGo apps back to docker-compose format.

use std::collections::HashMap;
use std::path::PathBuf;

use clap::{Args, Subcommand};
use colored::Colorize;
use serde::{Deserialize, Serialize};

use crate::CliConfig;

/// Compose command arguments
#[derive(Args)]
pub struct ComposeArgs {
    #[command(subcommand)]
    command: ComposeCommands,
}

/// Compose subcommands
#[derive(Subcommand)]
enum ComposeCommands {
    /// Import docker-compose.yml as ShellWeGo app
    Import {
        /// Path to docker-compose.yml
        #[arg(default_value = "docker-compose.yml")]
        file: PathBuf,
        
        /// App name prefix
        #[arg(short, long)]
        name: Option<String>,
        
        /// Organization ID
        #[arg(short, long)]
        org: Option<uuid::Uuid>,
        
        /// Dry run (show what would be created)
        #[arg(long)]
        dry_run: bool,
        
        /// Environment file to use
        #[arg(short, long)]
        env_file: Option<PathBuf>,
    },
    
    /// Convert ShellWeGo app to docker-compose.yml
    Export {
        /// App ID to export
        app_id: uuid::Uuid,
        
        /// Output file
        #[arg(short, long)]
        output: Option<PathBuf>,
        
        /// Include secrets as environment variables
        #[arg(long)]
        include_secrets: bool,
    },
    
    /// Validate docker-compose.yml compatibility
    Validate {
        /// Path to docker-compose.yml
        file: PathBuf,
        
        /// Show suggestions for unsupported features
        #[arg(long)]
        suggestions: bool,
    },
}

/// Handle compose command
pub async fn handle(args: ComposeArgs, config: &CliConfig) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    
    match args.command {
        ComposeCommands::Import { file, name, org, dry_run, env_file } => {
            handle_import(&client, config, file, name, org, dry_run, env_file).await
        }
        ComposeCommands::Export { app_id, output, include_secrets } => {
            handle_export(&client, config, app_id, output, include_secrets).await
        }
        ComposeCommands::Validate { file, suggestions } => {
            handle_validate(&client, file, suggestions).await
        }
    }
}

/// Handle docker-compose import
async fn handle_import(
    client: &reqwest::Client,
    config: &CliConfig,
    file: PathBuf,
    name: Option<String>,
    org: Option<uuid::Uuid>,
    dry_run: bool,
    env_file: Option<PathBuf>,
) -> anyhow::Result<()> {
    println!("{} Reading {}", "›".blue(), file.display());
    
    // Read compose file
    let content = tokio::fs::read_to_string(&file).await
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", file.display(), e))?;
    
    // Parse compose file
    let compose: DockerCompose = serde_yaml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse docker-compose: {}", e))?;
    
    let service_count = compose.services.len();
    println!("{} Found {} service(s)", "›".blue(), service_count);
    
    // Load environment file if provided
    let env_vars = if let Some(env_path) = env_file {
        load_env_file(&env_path).await?
    } else {
        HashMap::new()
    };
    
    let app_prefix = name.unwrap_or_else(|| {
        file.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("app")
            .to_string()
    });
    
    // Convert services to ShellWeGo apps
    let mut apps = Vec::new();
    for (service_name, service) in &compose.services {
        println!("  {} Converting service: {}", "•".dimmed(), service_name);
        
        let app = convert_service_to_app(
            &app_prefix,
            service_name,
            service,
            &env_vars,
            &compose.volumes,
            &compose.networks,
        )?;
        
        apps.push(app);
    }
    
    if dry_run {
        println!();
        println!("{} Dry run - would create:", "✓".yellow().bold());
        for app in &apps {
            println!("  {} {} (image: {})", 
                "•".dimmed(), 
                app.name, 
                app.image.as_deref().unwrap_or("build")
            );
        }
        return Ok(());
    }
    
    // Create apps via API
    let token = config.token.clone()
        .ok_or_else(|| anyhow::anyhow!("Not authenticated"))?;
    
    let org_id = org.or(config.default_org)
        .ok_or_else(|| anyhow::anyhow!("Organization required. Use --org or set default."))?;
    
    println!();
    println!("{} Creating apps...", "›".blue());
    
    for app in apps {
        let url = format!("{}/api/v1/apps", config.api_url);
        
        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&app)
            .send()
            .await?;
        
        if response.status().is_success() {
            println!("  {} Created {}", "✓".green(), app.name);
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            println!("  {} Failed to create {}: {} - {}", "✗".red(), app.name, status, body);
        }
    }
    
    println!();
    println!("{} Import complete", "✓".green().bold());
    
    Ok(())
}

/// Handle app export to docker-compose
async fn handle_export(
    client: &reqwest::Client,
    config: &CliConfig,
    app_id: uuid::Uuid,
    output: Option<PathBuf>,
    include_secrets: bool,
) -> anyhow::Result<()> {
    println!("{} Fetching app configuration...", "›".blue());
    
    let token = config.token.clone()
        .ok_or_else(|| anyhow::anyhow!("Not authenticated"))?;
    
    let url = format!("{}/api/v1/apps/{}", config.api_url, app_id);
    
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;
    
    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Failed to fetch app: {}", response.status()));
    }
    
    let app: serde_json::Value = response.json().await?;
    
    // Convert to docker-compose
    let compose = convert_app_to_compose(&app, include_secrets)?;
    
    let yaml = serde_yaml::to_string(&compose)?;
    
    if let Some(output_path) = output {
        tokio::fs::write(&output_path, &yaml).await?;
        println!("{} Exported to {}", "✓".green(), output_path.display());
    } else {
        println!("{}", yaml);
    }
    
    Ok(())
}

/// Handle compose validation
async fn handle_validate(
    _client: &reqwest::Client,
    file: PathBuf,
    suggestions: bool,
) -> anyhow::Result<()> {
    println!("{} Validating {}", "›".blue(), file.display());
    
    let content = tokio::fs::read_to_string(&file).await
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", file.display(), e))?;
    
    let compose: DockerCompose = serde_yaml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse docker-compose: {}", e))?;
    
    let mut issues = Vec::new();
    let mut warnings = Vec::new();
    
    // Check for unsupported features
    for (name, service) in &compose.services {
        // Check for privileged mode
        if service.privileged.unwrap_or(false) {
            issues.push(format!(
                "Service '{}' uses privileged mode - not supported",
                name
            ));
        }
        
        // Check for host networking
        if service.network_mode.as_deref() == Some("host") {
            issues.push(format!(
                "Service '{}' uses host networking - not supported",
                name
            ));
        }
        
        // Check for volume mounts to host paths
        for volume in &service.volumes {
            if volume.starts_with('/') || volume.starts_with("./") || volume.starts_with("../") {
                warnings.push(format!(
                    "Service '{}' mounts host path '{}' - will be converted to persistent volume",
                    name, volume
                ));
            }
        }
        
        // Check for depends_on conditions
        if let Some(depends) = &service.depends_on {
            if !depends.is_empty() {
                warnings.push(format!(
                    "Service '{}' uses depends_on - ShellWeGo handles this via service discovery",
                    name
                ));
            }
        }
        
        // Check for healthcheck
        if service.healthcheck.is_some() {
            warnings.push(format!(
                "Service '{}' has healthcheck - ShellWeGo has built-in health checks",
                name
            ));
        }
        
        // Check for init
        if service.init.unwrap_or(false) {
            issues.push(format!(
                "Service '{}' uses init - not currently supported",
                name
            ));
        }
        
        // Check for user
        if service.user.is_some() {
            warnings.push(format!(
                "Service '{}' specifies user - may need adjustment for ShellWeGo",
                name
            ));
        }
        
        // Check for cap_add/cap_drop
        if service.cap_add.is_some() || service.cap_drop.is_some() {
            issues.push(format!(
                "Service '{}' uses capabilities - not supported in multi-tenant environment",
                name
            ));
        }
        
        // Check for security_opt
        if service.security_opt.is_some() {
            warnings.push(format!(
                "Service '{}' uses security_opt - may need adjustment",
                name
            ));
        }
    }
    
    // Check for external networks
    for (name, network) in &compose.networks {
        if network.external == Some(true) {
            issues.push(format!(
                "Network '{}' is external - ShellWeGo creates its own networks",
                name
            ));
        }
    }
    
    // Check for external volumes
    for (name, volume) in &compose.volumes {
        if volume.external == Some(true) {
            issues.push(format!(
                "Volume '{}' is external - will need to be created in ShellWeGo",
                name
            ));
        }
    }
    
    // Print results
    println!();
    
    if issues.is_empty() && warnings.is_empty() {
        println!("{} No compatibility issues found", "✓".green().bold());
        println!("  This compose file should import cleanly.");
    } else {
        if !issues.is_empty() {
            println!("{} Issues (must be resolved):", "✗".red().bold());
            for issue in &issues {
                println!("  {} {}", "•".red(), issue);
            }
        }
        
        if !warnings.is_empty() {
            println!();
            println!("{} Warnings (may need attention):", "!".yellow().bold());
            for warning in &warnings {
                println!("  {} {}", "•".yellow(), warning);
            }
        }
        
        if suggestions {
            println!();
            println!("{} Suggestions:", "›".blue().bold());
            println!("  • Remove privileged mode and use non-root containers");
            println!("  • Replace host volumes with named volumes");
            println!("  • Use environment variables for configuration");
            println!("  • Let ShellWeGo handle networking and service discovery");
        }
    }
    
    if !issues.is_empty() {
        Err(anyhow::anyhow!("Validation failed with {} issue(s)", issues.len()))
    } else {
        Ok(())
    }
}

/// Load environment file
async fn load_env_file(path: &PathBuf) -> anyhow::Result<HashMap<String, String>> {
    let content = tokio::fs::read_to_string(path).await?;
    
    let mut vars = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        
        if let Some((key, value)) = line.split_once('=') {
            vars.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    
    Ok(vars)
}

/// Convert docker-compose service to ShellWeGo app
fn convert_service_to_app(
    prefix: &str,
    service_name: &str,
    service: &ComposeService,
    env_vars: &HashMap<String, String>,
    _volumes: &HashMap<String, ComposeVolume>,
    _networks: &HashMap<String, ComposeNetwork>,
) -> anyhow::Result<AppDefinition> {
    let name = format!("{}-{}", prefix, service_name);
    
    // Get image or build context
    let image = service.image.clone();
    
    // Get command
    let command = service.command.clone();
    
    // Get entrypoint
    let entrypoint = service.entrypoint.clone();
    
    // Get environment variables
    let mut env = HashMap::new();
    
    // From service environment
    for env_item in &service.environment {
        if let Some((key, value)) = env_item.split_once('=') {
            env.insert(key.to_string(), value.to_string());
        } else {
            // Check env_vars for value
            if let Some(value) = env_vars.get(env_item) {
                env.insert(env_item.to_string(), value.clone());
            }
        }
    }
    
    // From env_file
    for _env_file in &service.env_file {
        // Would load and merge env file
    }
    
    // Get ports
    let ports: Vec<String> = service.ports.iter()
        .map(|p| p.clone())
        .collect();
    
    // Get volume mounts
    let volume_mounts: Vec<VolumeMount> = service.volumes.iter()
        .filter_map(|v| parse_volume_spec(v))
        .collect();
    
    // Get resource limits
    let resources = service.deploy.as_ref().map(|d| ResourceSpec {
        cpu_limit: d.resources.as_ref().and_then(|r| r.limits.as_ref())
            .and_then(|l| l.cpus.clone())
            .and_then(|c| c.parse().ok()),
        memory_limit: service.deploy.as_ref().and_then(|d| 
            d.resources.as_ref().and_then(|r| r.limits.as_ref())
                .and_then(|l| l.memory.clone())
        ),
    }).unwrap_or_default();
    
    Ok(AppDefinition {
        name,
        image,
        build: service.build.clone().map(|b| BuildSpec {
            context: b.context.unwrap_or_else(|| ".".to_string()),
            dockerfile: b.dockerfile,
            args: b.args.unwrap_or_default(),
        }),
        command,
        entrypoint,
        environment: env,
        ports,
        volumes: volume_mounts,
        resources,
        replicas: service.deploy.as_ref()
            .and_then(|d| d.replicas)
            .unwrap_or(1),
        health_check_path: service.healthcheck.as_ref()
            .and_then(|h| h.test.as_ref())
            .and_then(|t| t.first().cloned()),
    })
}

/// Parse volume specification
fn parse_volume_spec(spec: &str) -> Option<VolumeMount> {
    let parts: Vec<&str> = spec.split(':').collect();
    
    if parts.len() >= 2 {
        Some(VolumeMount {
            source: parts[0].to_string(),
            destination: parts[1].to_string(),
            read_only: parts.get(2).map(|&s| s.contains("ro")).unwrap_or(false),
        })
    } else {
        None
    }
}

/// Convert ShellWeGo app to docker-compose
fn convert_app_to_compose(app: &serde_json::Value, include_secrets: bool) -> anyhow::Result<DockerCompose> {
    let mut services = HashMap::new();
    
    let app_name = app["name"].as_str().unwrap_or("app");
    let image = app["image"].as_str().map(|s| s.to_string());
    
    let mut service = ComposeService {
        image,
        build: None,
        command: app["command"].as_str().map(|s| s.to_string()),
        entrypoint: None,
        environment: Vec::new(),
        env_file: None,
        ports: Vec::new(),
        volumes: Vec::new(),
        depends_on: HashMap::new(),
        networks: Vec::new(),
        privileged: None,
        user: None,
        working_dir: None,
        healthcheck: None,
        deploy: None,
        init: None,
        cap_add: None,
        cap_drop: None,
        security_opt: None,
        labels: None,
        restart: Some("unless-stopped".to_string()),
    };
    
    // Add environment variables
    if let Some(env) = app["environment"].as_object() {
        for (key, value) in env {
            if include_secrets || !key.starts_with("SECRET_") {
                service.environment.push(format!("{}={}", key, 
                    value.as_str().unwrap_or("")
                ));
            }
        }
    }
    
    // Add ports
    if let Some(ports) = app["ports"].as_array() {
        for port in ports {
            service.ports.push(port.as_str().unwrap_or("").to_string());
        }
    }
    
    // Add volumes
    if let Some(volumes) = app["volumes"].as_array() {
        for vol in volumes {
            if let Some(vol_str) = vol.as_str() {
                service.volumes.push(vol_str.to_string());
            }
        }
    }
    
    // Add resources
    if let Some(resources) = app["resources"].as_object() {
        service.deploy = Some(DeploySpec {
            replicas: app["replicas"].as_u64().map(|r| r as u32),
            resources: Some(Resources {
                limits: Some(ResourceLimits {
                    cpus: resources["cpu_limit"].as_str().map(|s| s.to_string()),
                    memory: resources["memory_limit"].as_str().map(|s| s.to_string()),
                }),
                reservations: None,
            }),
            placement: None,
            update_config: None,
        });
    }
    
    services.insert(app_name.to_string(), service);
    
    Ok(DockerCompose {
        version: "3.8".to_string(),
        services,
        volumes: HashMap::new(),
        networks: HashMap::new(),
    })
}

// --- Data Structures ---

/// App definition for creation
#[derive(Debug, Serialize)]
pub struct AppDefinition {
    pub name: String,
    pub image: Option<String>,
    pub build: Option<BuildSpec>,
    pub command: Option<String>,
    pub entrypoint: Option<String>,
    pub environment: HashMap<String, String>,
    pub ports: Vec<String>,
    pub volumes: Vec<VolumeMount>,
    pub resources: ResourceSpec,
    pub replicas: u32,
    pub health_check_path: Option<String>,
}

/// Build specification
#[derive(Debug, Serialize, Deserialize)]
pub struct BuildSpec {
    pub context: String,
    pub dockerfile: Option<String>,
    pub args: HashMap<String, String>,
}

/// Volume mount
#[derive(Debug, Serialize, Deserialize)]
pub struct VolumeMount {
    pub source: String,
    pub destination: String,
    pub read_only: bool,
}

/// Resource specification
#[derive(Debug, Default, Serialize)]
pub struct ResourceSpec {
    pub cpu_limit: Option<f64>,
    pub memory_limit: Option<String>,
}

/// Docker Compose file structure
#[derive(Debug, Serialize, Deserialize)]
pub struct DockerCompose {
    pub version: String,
    pub services: HashMap<String, ComposeService>,
    #[serde(default)]
    pub volumes: HashMap<String, ComposeVolume>,
    #[serde(default)]
    pub networks: HashMap<String, ComposeNetwork>,
}

/// Compose service definition
#[derive(Debug, Serialize, Deserialize)]
pub struct ComposeService {
    pub image: Option<String>,
    pub build: Option<ComposeBuild>,
    pub command: Option<String>,
    pub entrypoint: Option<String>,
    #[serde(default)]
    pub environment: Vec<String>,
    pub env_file: Option<String>,
    #[serde(default)]
    pub ports: Vec<String>,
    #[serde(default)]
    pub volumes: Vec<String>,
    #[serde(default)]
    pub depends_on: HashMap<String, ComposeDependsOn>,
    #[serde(default)]
    pub networks: Vec<String>,
    pub privileged: Option<bool>,
    pub user: Option<String>,
    pub working_dir: Option<String>,
    pub healthcheck: Option<HealthCheck>,
    pub deploy: Option<DeploySpec>,
    pub init: Option<bool>,
    pub cap_add: Option<Vec<String>>,
    pub cap_drop: Option<Vec<String>>,
    pub security_opt: Option<Vec<String>>,
    pub labels: Option<HashMap<String, String>>,
    pub restart: Option<String>,
}

/// Compose build specification
#[derive(Debug, Serialize, Deserialize)]
pub struct ComposeBuild {
    pub context: Option<String>,
    pub dockerfile: Option<String>,
    pub args: Option<HashMap<String, String>>,
}

/// Compose dependency condition
#[derive(Debug, Serialize, Deserialize)]
pub struct ComposeDependsOn {
    pub condition: String,
}

/// Health check configuration
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthCheck {
    pub test: Option<Vec<String>>,
    pub interval: Option<String>,
    pub timeout: Option<String>,
    pub retries: Option<u32>,
    pub start_period: Option<String>,
}

/// Deploy specification
#[derive(Debug, Serialize, Deserialize)]
pub struct DeploySpec {
    pub replicas: Option<u32>,
    pub resources: Option<Resources>,
    pub placement: Option<Placement>,
    pub update_config: Option<UpdateConfig>,
}

/// Resource constraints
#[derive(Debug, Serialize, Deserialize)]
pub struct Resources {
    pub limits: Option<ResourceLimits>,
    pub reservations: Option<ResourceLimits>,
}

/// Resource limits
#[derive(Debug, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub cpus: Option<String>,
    pub memory: Option<String>,
}

/// Placement constraints
#[derive(Debug, Serialize, Deserialize)]
pub struct Placement {
    pub constraints: Vec<String>,
}

/// Update configuration
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateConfig {
    pub parallelism: u32,
    pub delay: Option<String>,
    pub failure_action: Option<String>,
}

/// Volume definition
#[derive(Debug, Serialize, Deserialize)]
pub struct ComposeVolume {
    pub driver: Option<String>,
    pub external: Option<bool>,
}

/// Network definition
#[derive(Debug, Serialize, Deserialize)]
pub struct ComposeNetwork {
    pub driver: Option<String>,
    pub external: Option<bool>,
}
