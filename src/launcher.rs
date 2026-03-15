use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{broadcast, mpsc};

use crate::agent::AgentState;
use crate::config::SimConfig;
use crate::engine::{
    EngineControls, SharedState, SimStatus, SimulationEngine, WsEvent,
};
use crate::llm::LlmClient;
use crate::world::WorldState;
use crate::{god_eye, parser};

#[derive(Debug, serde::Deserialize)]
pub struct LaunchRequest {
    pub scenario_prompt: String,
    pub total_rounds: Option<u32>,
    pub seed_document_text: Option<String>,
}

pub async fn launch_simulation(
    req: LaunchRequest,
    base_config: &SimConfig,
    llm: Arc<LlmClient>,
    state: SharedState,
    ws_tx: broadcast::Sender<WsEvent>,
) -> anyhow::Result<EngineControls> {
    // Build config from base + overrides
    let mut config = base_config.clone();
    config.simulation.scenario_prompt = req.scenario_prompt;
    if let Some(rounds) = req.total_rounds {
        config.simulation.total_rounds = rounds;
    }

    // Parse seed documents
    let mut documents = Vec::new();

    // If seed text provided, use it directly
    if let Some(text) = &req.seed_document_text {
        if !text.trim().is_empty() {
            let chunks =
                parser::chunk_text(text, config.parser.chunk_size, config.parser.chunk_overlap);
            documents.extend(chunks);
        }
    }

    // Also parse configured seed documents if no inline text
    if documents.is_empty() {
        for doc_path in &config.simulation.seed_documents {
            match parser::parse_document(doc_path, config.parser.max_chars_per_doc) {
                Ok(text) => {
                    let chunks = parser::chunk_text(
                        &text,
                        config.parser.chunk_size,
                        config.parser.chunk_overlap,
                    );
                    documents.extend(chunks);
                }
                Err(e) => tracing::warn!("Skipping {}: {e}", doc_path.display()),
            }
        }
    }

    // If still no documents, use the scenario prompt itself
    if documents.is_empty() {
        documents.push(config.simulation.scenario_prompt.clone());
    }

    // Extract entities and generate agents
    tracing::info!("Launcher: extracting entities...");
    let agents =
        parser::extract_and_generate_agents(&llm, &documents, &config.simulation.scenario_prompt)
            .await?;

    tracing::info!("Launcher: generated {} agents", agents.len());

    // Build agent maps
    let mut agents_map = HashMap::new();
    let mut agent_states_map = HashMap::new();
    for a in agents {
        let id = a.id;
        agent_states_map.insert(id, AgentState::new(id));
        agents_map.insert(id, a);
    }

    // Replace shared state contents
    {
        let mut s = state.write().await;
        s.status = SimStatus::Preparing;
        s.agents = agents_map;
        s.agent_states = agent_states_map;
        s.world = WorldState::new(chrono::Utc::now());
        s.config = config.clone();
        s.total_actions = 0;
        s.syntheses = Vec::new();
    }

    // Create new control channels
    let (god_eye_tx, god_eye_rx) = mpsc::channel(100);
    let (pause_tx, pause_rx) = mpsc::channel(1);
    let (resume_tx, resume_rx) = mpsc::channel(1);
    let (stop_tx, stop_rx) = mpsc::channel(1);

    let controls = EngineControls {
        pause_tx,
        resume_tx,
        stop_tx,
        god_eye_tx: god_eye_tx.clone(),
    };

    // Start God's Eye watcher
    if config.god_eye.enabled {
        if let Err(e) = god_eye::start_watcher(
            config.god_eye.events_file.clone(),
            config.god_eye.debounce_ms,
            god_eye_tx,
        ) {
            tracing::warn!("God's Eye watcher failed: {e}");
        }
    }

    // Spawn engine task
    let engine_state = state.clone();
    let engine_llm = llm.clone();
    let engine_ws_tx = ws_tx.clone();
    let verbose = config.output.verbose;
    let output_dir = config.output.output_dir.clone();
    let report_file = config.output.report_file.clone();

    tokio::spawn(async move {
        let mut engine = SimulationEngine::new(
            engine_state.clone(),
            engine_llm.clone(),
            god_eye_rx,
            engine_ws_tx,
            pause_rx,
            resume_rx,
            stop_rx,
        );

        match engine.run(verbose).await {
            Ok(_) => {
                tracing::info!("Launched simulation completed");
                let s = engine_state.read().await;
                match crate::report::save_report(&engine_llm, &*s, &output_dir, &report_file).await
                {
                    Ok(path) => tracing::info!("Report saved: {path}"),
                    Err(e) => tracing::warn!("Report generation failed: {e}"),
                }
            }
            Err(e) => tracing::error!("Launched simulation failed: {e}"),
        }
    });

    Ok(controls)
}
