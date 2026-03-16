use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use crate::agent::{AgentState, Tier};
use crate::config::SimConfig;
use crate::engine::{
    EngineControls, SharedState, SimStatus, SimulationEngine, SimulationState, WsEvent,
};
use crate::llm::LlmClient;
use crate::world::WorldState;
use crate::{god_eye, parser};

#[derive(Debug, serde::Deserialize)]
pub struct LaunchRequest {
    pub scenario_prompt: String,
    pub total_rounds: Option<u32>,
    pub seed_document_text: Option<String>,
    pub target_agent_count: Option<u32>,
    pub challenge_question: Option<String>,
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
    config.simulation.challenge_question = req.challenge_question;
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
    let target_agents = req.target_agent_count.unwrap_or(40) as usize;
    tracing::info!("Launcher: extracting stakeholders (target: {target_agents} agents)...");
    let agents =
        parser::extract_and_generate_agents(&llm, &documents, &config.simulation.scenario_prompt, target_agents)
            .await?;

    tracing::info!("Launcher: generated {} agents", agents.len());

    // Build agent maps
    let mut agents_map = HashMap::new();
    let mut agent_states_map = HashMap::new();
    for a in agents {
        let id = a.id;
        agent_states_map.insert(id, AgentState::new_with_sentiment(id, a.sentiment_bias));
        agents_map.insert(id, a);
    }

    // Reset token counters for new simulation
    llm.reset_tokens();

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
        s.prompt_tokens = 0;
        s.completion_tokens = 0;

        // Seed social graph
        seed_social_graph(&mut s);
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

/// Continue a finished simulation for additional rounds without regenerating agents.
pub async fn continue_simulation(
    extra_rounds: u32,
    state: SharedState,
    llm: Arc<LlmClient>,
    ws_tx: broadcast::Sender<WsEvent>,
) -> anyhow::Result<EngineControls> {
    let config = {
        let s = state.read().await;
        s.config.clone()
    };

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

    // Restart God's Eye watcher
    if config.god_eye.enabled {
        if let Err(e) = god_eye::start_watcher(
            config.god_eye.events_file.clone(),
            config.god_eye.debounce_ms,
            god_eye_tx,
        ) {
            tracing::warn!("God's Eye watcher failed: {e}");
        }
    }

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

        match engine.run_continuation(extra_rounds, verbose).await {
            Ok(_) => {
                tracing::info!("Continuation ({extra_rounds} rounds) completed");
                let s = engine_state.read().await;
                match crate::report::save_report(&engine_llm, &*s, &output_dir, &report_file).await
                {
                    Ok(path) => tracing::info!("Report saved: {path}"),
                    Err(e) => tracing::warn!("Report generation failed: {e}"),
                }
            }
            Err(e) => tracing::error!("Continuation failed: {e}"),
        }
    });

    Ok(controls)
}

/// Pre-populate the social graph so agents start with followers/following from round 1.
pub fn seed_social_graph(state: &mut SimulationState) {
    let tier1_ids: Vec<Uuid> = state
        .agents
        .iter()
        .filter(|(_, a)| matches!(a.tier, Tier::Tier1))
        .map(|(id, _)| *id)
        .collect();

    let tier2_ids: Vec<Uuid> = state
        .agents
        .iter()
        .filter(|(_, a)| matches!(a.tier, Tier::Tier2))
        .map(|(id, _)| *id)
        .collect();

    let all_ids: Vec<Uuid> = state.agents.keys().cloned().collect();

    // All agents follow all Tier1 VIPs (public figures)
    for &vip_id in &tier1_ids {
        for &agent_id in &all_ids {
            if agent_id == vip_id {
                continue;
            }
            state.world.social_graph.add_follow(agent_id, vip_id);
            if let Some(agent_state) = state.agent_states.get_mut(&agent_id) {
                if !agent_state.following.contains(&vip_id) {
                    agent_state.following.push(vip_id);
                }
            }
            if let Some(vip_state) = state.agent_states.get_mut(&vip_id) {
                if !vip_state.followers.contains(&agent_id) {
                    vip_state.followers.push(agent_id);
                }
            }
        }
    }

    // Tier2 agents follow each other with 60% probability
    for i in 0..tier2_ids.len() {
        for j in 0..tier2_ids.len() {
            if i == j {
                continue;
            }
            let roll: f32 = rand::random();
            if roll < 0.6 {
                let follower = tier2_ids[i];
                let target = tier2_ids[j];
                if !state.world.social_graph.is_following(&follower, &target) {
                    state.world.social_graph.add_follow(follower, target);
                    if let Some(st) = state.agent_states.get_mut(&follower) {
                        if !st.following.contains(&target) {
                            st.following.push(target);
                        }
                    }
                    if let Some(st) = state.agent_states.get_mut(&target) {
                        if !st.followers.contains(&follower) {
                            st.followers.push(follower);
                        }
                    }
                }
            }
        }
    }

    // Tier3 figurants follow 3-8 random agents
    let non_tier1_non_self: Vec<Uuid> = all_ids.clone();
    for &agent_id in &all_ids {
        let profile = match state.agents.get(&agent_id) {
            Some(p) => p,
            None => continue,
        };
        if !matches!(profile.tier, Tier::Tier3) {
            continue;
        }
        // Already follows all T1 from above, now add 3-8 more random follows
        let extra_follows: usize = 3 + (rand::random::<usize>() % 6);
        let mut followed_count = 0;
        for &target_id in &non_tier1_non_self {
            if followed_count >= extra_follows {
                break;
            }
            if target_id == agent_id || tier1_ids.contains(&target_id) {
                continue;
            }
            let roll: f32 = rand::random();
            if roll < 0.15 {
                if !state.world.social_graph.is_following(&agent_id, &target_id) {
                    state.world.social_graph.add_follow(agent_id, target_id);
                    if let Some(st) = state.agent_states.get_mut(&agent_id) {
                        if !st.following.contains(&target_id) {
                            st.following.push(target_id);
                        }
                    }
                    if let Some(st) = state.agent_states.get_mut(&target_id) {
                        if !st.followers.contains(&agent_id) {
                            st.followers.push(agent_id);
                        }
                    }
                    followed_count += 1;
                }
            }
        }
    }

    let total_follows: usize = state.world.social_graph.following.values().map(|v| v.len()).sum();
    tracing::info!("Seeded social graph: {} follow relationships", total_follows);
}
