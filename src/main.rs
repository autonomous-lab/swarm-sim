mod agent;
mod config;
mod engine;
mod god_eye;
mod launcher;
mod llm;
mod output;
mod parser;
mod report;
mod server;
mod world;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use tokio::sync::{broadcast, mpsc, RwLock};

use crate::agent::AgentState;
use crate::config::SimConfig;
use crate::engine::{EngineControls, SimStatus, SimulationEngine, SimulationState};
use crate::llm::LlmClient;
use crate::world::WorldState;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "swarm-sim",
    version,
    about = "Multi-agent social simulation engine with tiered LLM batching"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a full simulation
    Run {
        /// Path to config.toml
        #[arg(short, long, default_value = "config.toml")]
        config: PathBuf,
        /// Override total rounds
        #[arg(long)]
        rounds: Option<u32>,
        /// Override output directory
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Start web server only (launch simulations from UI)
    Server {
        /// Path to config.toml
        #[arg(short, long, default_value = "config.toml")]
        config: PathBuf,
    },
    /// Extract entities from seed documents (dry run)
    Extract {
        #[arg(short, long, default_value = "config.toml")]
        config: PathBuf,
        /// Output entity list as JSON
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Generate a report from an existing simulation state
    Report {
        #[arg(short, long, default_value = "config.toml")]
        config: PathBuf,
        #[arg(short, long, default_value = "report.md")]
        output: PathBuf,
    },
    /// Validate a config file
    Validate {
        #[arg(short, long, default_value = "config.toml")]
        config: PathBuf,
    },
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Init tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "swarm_sim=info".into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Validate { config } => cmd_validate(&config),
        Commands::Extract { config, output } => cmd_extract(&config, output.as_deref()).await,
        Commands::Report { config, output } => cmd_report(&config, &output).await,
        Commands::Server { config } => cmd_server(&config).await,
        Commands::Run {
            config,
            rounds,
            output,
        } => cmd_run(&config, rounds, output.as_deref()).await,
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn cmd_validate(config_path: &std::path::Path) -> anyhow::Result<()> {
    match SimConfig::load(config_path) {
        Ok(config) => {
            println!("Config is valid.");
            println!("  Rounds: {}", config.simulation.total_rounds);
            println!("  Seed docs: {:?}", config.simulation.seed_documents);
            println!(
                "  Tiers: T1(batch={}) T2(batch={}) T3(batch={})",
                config.tiers.tier1.batch_size,
                config.tiers.tier2.batch_size,
                config.tiers.tier3.batch_size,
            );
            println!(
                "  Server: {}:{} ({})",
                config.server.host,
                config.server.port,
                if config.server.enabled {
                    "enabled"
                } else {
                    "disabled"
                },
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("Config validation failed: {e}");
            std::process::exit(1);
        }
    }
}

async fn cmd_extract(
    config_path: &std::path::Path,
    output: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    let config = SimConfig::load(config_path)?;

    let llm = LlmClient::new(
        &config.tiers,
        config.llm.extraction_model.clone(),
        config.llm.extraction_base_url.clone(),
        config.llm.extraction_api_key.clone(),
    );

    let mut documents = Vec::new();
    for doc_path in &config.simulation.seed_documents {
        match parser::parse_document(doc_path, config.parser.max_chars_per_doc) {
            Ok(text) => {
                let chunks =
                    parser::chunk_text(&text, config.parser.chunk_size, config.parser.chunk_overlap);
                documents.extend(chunks);
            }
            Err(e) => tracing::warn!("Failed to parse {}: {e}", doc_path.display()),
        }
    }

    let agents = parser::extract_and_generate_agents(
        &llm,
        &documents,
        &config.simulation.scenario_prompt,
        40,
    )
    .await?;

    println!("Extracted {} agents:", agents.len());
    for a in &agents {
        println!("  [{:?}] @{} — {} ({})", a.tier, a.username, a.name, a.stance);
    }

    if let Some(out_path) = output {
        let json = serde_json::to_string_pretty(&agents)?;
        std::fs::write(out_path, json)?;
        println!("\nSaved to {}", out_path.display());
    }

    Ok(())
}

async fn cmd_report(
    config_path: &std::path::Path,
    output: &std::path::Path,
) -> anyhow::Result<()> {
    let config = SimConfig::load(config_path)?;

    let llm = LlmClient::new(
        &config.tiers,
        config.llm.extraction_model.clone(),
        config.llm.extraction_base_url.clone(),
        config.llm.extraction_api_key.clone(),
    );

    let state = SimulationState {
        status: SimStatus::Finished,
        agents: HashMap::new(),
        agent_states: HashMap::new(),
        world: WorldState::new(chrono::Utc::now()),
        config: config.clone(),
        total_actions: 0,
        syntheses: Vec::new(),
        prompt_tokens: 0,
        completion_tokens: 0,
    };

    let report_text = report::generate_report(&llm, &state).await?;
    std::fs::write(output, &report_text)?;
    println!("Report saved to {}", output.display());
    Ok(())
}

/// Start web server only — simulations launched from UI.
async fn cmd_server(config_path: &std::path::Path) -> anyhow::Result<()> {
    output::print_banner();

    let config = SimConfig::load(config_path)?;

    let llm = Arc::new(LlmClient::new(
        &config.tiers,
        config.llm.extraction_model.clone(),
        config.llm.extraction_base_url.clone(),
        config.llm.extraction_api_key.clone(),
    ));

    // Empty state — Idle, waiting for launch
    let state = Arc::new(RwLock::new(SimulationState {
        status: SimStatus::Idle,
        agents: HashMap::new(),
        agent_states: HashMap::new(),
        world: WorldState::new(chrono::Utc::now()),
        config: config.clone(),
        total_actions: 0,
        syntheses: Vec::new(),
        prompt_tokens: 0,
        completion_tokens: 0,
    }));

    let (ws_tx, _) = broadcast::channel::<engine::WsEvent>(1024);
    let (god_eye_tx, _god_eye_rx) = mpsc::channel(100);
    let (pause_tx, _pause_rx) = mpsc::channel(1);
    let (resume_tx, _resume_rx) = mpsc::channel(1);
    let (stop_tx, _stop_rx) = mpsc::channel(1);

    let controls = Arc::new(RwLock::new(EngineControls {
        pause_tx,
        resume_tx,
        stop_tx,
        god_eye_tx,
    }));

    let web_dir = std::env::current_dir()
        .unwrap_or_default()
        .join("web")
        .to_string_lossy()
        .to_string();

    let app_state = server::AppState {
        sim_state: state,
        controls,
        ws_tx,
        llm,
        base_config: Arc::new(config.clone()),
    };

    println!("Server mode — launch simulations from the Web UI");
    println!("  http://{}:{}", config.server.host, config.server.port);

    server::start(
        &config.server.host,
        config.server.port,
        app_state,
        &web_dir,
        false,
    )
    .await
}

async fn cmd_run(
    config_path: &std::path::Path,
    override_rounds: Option<u32>,
    override_output: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    output::print_banner();

    let mut config = SimConfig::load(config_path)?;
    if let Some(rounds) = override_rounds {
        config.simulation.total_rounds = rounds;
    }
    if let Some(out) = override_output {
        config.output.output_dir = out.to_path_buf();
    }

    let llm = Arc::new(LlmClient::new(
        &config.tiers,
        config.llm.extraction_model.clone(),
        config.llm.extraction_base_url.clone(),
        config.llm.extraction_api_key.clone(),
    ));

    // --- Phase 1: Parse documents & generate agents ---
    tracing::info!("Phase 1: Parsing seed documents...");
    let mut documents = Vec::new();
    for doc_path in &config.simulation.seed_documents {
        match parser::parse_document(doc_path, config.parser.max_chars_per_doc) {
            Ok(text) => {
                let chunks =
                    parser::chunk_text(&text, config.parser.chunk_size, config.parser.chunk_overlap);
                documents.extend(chunks);
            }
            Err(e) => tracing::warn!("Skipping {}: {e}", doc_path.display()),
        }
    }

    tracing::info!("Phase 2: Extracting entities & generating agents...");
    let agents = parser::extract_and_generate_agents(
        &llm,
        &documents,
        &config.simulation.scenario_prompt,
        40,
    )
    .await?;

    let tier1_count = agents.iter().filter(|a| matches!(a.tier, agent::Tier::Tier1)).count();
    let tier2_count = agents.iter().filter(|a| matches!(a.tier, agent::Tier::Tier2)).count();
    let tier3_count = agents.iter().filter(|a| matches!(a.tier, agent::Tier::Tier3)).count();

    tracing::info!(
        "Generated {} agents: {} VIP, {} standard, {} figurants",
        agents.len(),
        tier1_count,
        tier2_count,
        tier3_count,
    );

    // --- Phase 2: Build shared state ---
    let mut agents_map = HashMap::new();
    let mut agent_states_map = HashMap::new();
    for a in agents {
        let id = a.id;
        agent_states_map.insert(id, AgentState::new_with_sentiment(id, a.sentiment_bias));
        agents_map.insert(id, a);
    }

    let mut sim_state = SimulationState {
        status: SimStatus::Preparing,
        agents: agents_map,
        agent_states: agent_states_map,
        world: WorldState::new(chrono::Utc::now()),
        config: config.clone(),
        total_actions: 0,
        syntheses: Vec::new(),
        prompt_tokens: 0,
        completion_tokens: 0,
    };

    // Seed social graph before simulation starts
    launcher::seed_social_graph(&mut sim_state);

    let state = Arc::new(RwLock::new(sim_state));

    // --- Phase 3: Setup channels ---
    let (ws_tx, _) = broadcast::channel::<engine::WsEvent>(1024);
    let (god_eye_tx, god_eye_rx) = mpsc::channel(100);
    let (pause_tx, pause_rx) = mpsc::channel(1);
    let (resume_tx, resume_rx) = mpsc::channel(1);
    let (stop_tx, stop_rx) = mpsc::channel(1);

    let controls = Arc::new(RwLock::new(EngineControls {
        pause_tx,
        resume_tx,
        stop_tx,
        god_eye_tx: god_eye_tx.clone(),
    }));

    // --- Phase 4: Start God's Eye file watcher ---
    if config.god_eye.enabled {
        tracing::info!("Starting God's Eye watcher on {:?}", config.god_eye.events_file);
        if let Err(e) = god_eye::start_watcher(
            config.god_eye.events_file.clone(),
            config.god_eye.debounce_ms,
            god_eye_tx,
        ) {
            tracing::warn!("God's Eye watcher failed to start: {e}");
        }
    }

    // --- Phase 5: Start web server ---
    let web_dir = std::env::current_dir()
        .unwrap_or_default()
        .join("web")
        .to_string_lossy()
        .to_string();

    if config.server.enabled {
        let server_state = server::AppState {
            sim_state: state.clone(),
            controls: controls.clone(),
            ws_tx: ws_tx.clone(),
            llm: llm.clone(),
            base_config: Arc::new(config.clone()),
        };
        let host = config.server.host.clone();
        let port = config.server.port;
        let web_dir_clone = web_dir.clone();
        tokio::spawn(async move {
            if let Err(e) = server::start(&host, port, server_state, &web_dir_clone, false).await {
                tracing::error!("Web server error: {e}");
            }
        });
    }

    // --- Phase 6: Run simulation ---
    tracing::info!(
        "Starting simulation: {} rounds, {} agents",
        config.simulation.total_rounds,
        state.read().await.agents.len(),
    );

    let mut engine = SimulationEngine::new(
        state.clone(),
        llm.clone(),
        god_eye_rx,
        ws_tx,
        pause_rx,
        resume_rx,
        stop_rx,
    );

    engine.run(config.output.verbose).await?;

    // --- Phase 7: Generate report ---
    tracing::info!("Generating report...");
    let report_path = {
        let sim_state = state.read().await;
        match report::save_report(
            &llm,
            &sim_state,
            &config.output.output_dir,
            &config.output.report_file,
        )
        .await
        {
            Ok(path) => Some(path),
            Err(e) => {
                tracing::warn!("Report generation failed: {e}");
                None
            }
        }
        // Read lock dropped here
    };
    if let Some(ref path) = report_path {
        tracing::info!("Report saved: {path}");
    }

    println!("\nSimulation complete.");
    println!("  Actions log: {}", config.output.output_dir.join(&config.output.action_log).display());
    if let Some(ref path) = report_path {
        println!("  Report: {path}");
    }

    // Keep web server alive after simulation so users can explore results
    if config.server.enabled {
        println!("  Web UI still running at http://{}:{} — press Ctrl+C to stop.", config.server.host, config.server.port);
        tokio::signal::ctrl_c().await.ok();
        println!("\nShutting down.");
    }

    Ok(())
}
