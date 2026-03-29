use std::collections::HashMap;
use std::sync::Arc;

use rand::seq::SliceRandom;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use crate::agent::{AgentState, Tier};
use crate::config::SimConfig;
use crate::engine::{
    EngineControls, SharedState, SimStatus, SimulationEngine, SimulationState, WsEvent,
};
use crate::llm::LlmClient;
use crate::trial::{TrialSchedule, TrialState};
use crate::world::WorldState;
use crate::{god_eye, parser};

#[derive(Debug, serde::Deserialize)]
pub struct LaunchRequest {
    pub scenario_prompt: String,
    pub total_rounds: Option<u32>,
    pub seed_document_text: Option<String>,
    pub target_agent_count: Option<u32>,
    pub challenge_question: Option<String>,
    /// Simulation mode: "standard", "what_if", "crisis", "policy", "brand", "research".
    /// Each mode adjusts agent behavior and metrics focus.
    #[serde(default = "default_mode")]
    pub mode: String,
    /// For "what_if" mode: the intervention to test.
    pub what_if_intervention: Option<String>,
    /// For "research" mode: fixed random seed for reproducibility.
    pub research_seed: Option<u64>,
}

fn default_mode() -> String { "standard".to_string() }

pub async fn launch_simulation(
    req: LaunchRequest,
    base_config: &SimConfig,
    llm: Arc<LlmClient>,
    state: SharedState,
    ws_tx: broadcast::Sender<WsEvent>,
) -> anyhow::Result<EngineControls> {
    // Build config from base + overrides
    let mut config = base_config.clone();
    config.simulation.challenge_question = req.challenge_question;
    if let Some(rounds) = req.total_rounds {
        config.simulation.total_rounds = rounds;
    }
    if let Some(seed) = req.research_seed {
        config.simulation.random_seed = seed;
    }

    // Apply mode-specific scenario augmentation
    config.simulation.scenario_prompt = match req.mode.as_str() {
        "crisis" => format!(
            "[CRISIS SIMULATION] {}\n\nThis is an ACUTE CRISIS. Agents should show heightened emotions, \
             urgency, fear, anger, confusion. Information spreads rapidly. Misinformation is likely. \
             Some agents panic, others try to calm the situation. Official channels compete with rumors.",
            req.scenario_prompt
        ),
        "what_if" => {
            let intervention = req.what_if_intervention.as_deref().unwrap_or("an unspecified change");
            format!(
                "[WHAT-IF SCENARIO] Base scenario: {}\n\nINTERVENTION: {}\n\n\
                 Agents must react to this specific intervention. How does this change shift opinions, \
                 alliances, and behavior compared to the base scenario?",
                req.scenario_prompt, intervention
            )
        },
        "policy" => format!(
            "[POLICY TEST] {}\n\nThis is a POLICY ANNOUNCEMENT being tested on public opinion. \
             Focus on: stakeholder reactions, public acceptance, concerns raised, potential backlash, \
             and constructive feedback. Agents should evaluate trade-offs, not just react emotionally.",
            req.scenario_prompt
        ),
        "brand" => format!(
            "[BRAND REPUTATION ANALYSIS] {}\n\nFocus on brand perception: loyalty defenders vs critics, \
             customer sentiment shifts, competitor reactions, PR impact, backlash patterns, and recovery potential. \
             Track how brand advocates and detractors mobilize.",
            req.scenario_prompt
        ),
        "research" => format!(
            "[REPRODUCIBLE RESEARCH RUN] {}\n\nThis is a controlled research simulation. \
             Agents should behave consistently with their profiles. Focus on measurable dynamics: \
             opinion diffusion speed, cascade patterns, polarization evolution, and influence concentration.",
            req.scenario_prompt
        ),
        "trial" => format!(
            "[TRIAL SIMULATION] {}\n\nThis is a COURTROOM TRIAL. The LLM must generate court participants: \
             1 Judge (tier1), 1 Prosecutor + 1 Defense Attorney + 2-4 Witnesses (tier2), 12 Jurors (tier3). \
             The scenario describes the case being tried. Generate realistic legal proceedings.",
            req.scenario_prompt
        ),
        _ => req.scenario_prompt, // "standard" or unknown
    };

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
        // Start simulated time at 9:00 AM today so agents have full active_hours runway
        let today = chrono::Utc::now().date_naive();
        let start_time = today.and_hms_opt(9, 0, 0).unwrap().and_utc();
        s.world = WorldState::new(start_time);
        s.config = config.clone();
        // Use 30 min/round for web-launched sims (more rounds before agents "sleep")
        s.config.simulation.minutes_per_round = 30;
        s.total_actions = 0;
        s.syntheses = Vec::new();
        s.prompt_tokens = 0;
        s.completion_tokens = 0;
        s.trial = None;

        // Initialize trial state if trial mode
        if req.mode == "trial" {
            let schedule = TrialSchedule::default();
            s.config.simulation.total_rounds = schedule.total_rounds();
            let mut trial_state = TrialState::new(schedule);

            // Dedup names: collect names used by Tier1 + Tier2 (court officers)
            let reserved_names: std::collections::HashSet<String> = s.agents.iter()
                .filter(|(_, p)| matches!(p.tier, Tier::Tier1 | Tier::Tier2))
                .map(|(_, p)| p.name.to_lowercase())
                .collect();

            // Rename any Tier3 agent whose name collides with a court officer
            let tier3_ids: Vec<Uuid> = s.agents.iter()
                .filter(|(_, p)| matches!(p.tier, Tier::Tier3))
                .map(|(id, _)| *id)
                .collect();

            let rename_pool = [
                "Alex Thompson", "Carmen Rivera", "James Park", "Sofia Andersson",
                "Marcus Johnson", "Elena Petrova", "Daniel Kim", "Olivia Williams",
                "Hassan Ahmed", "Grace Chen", "Thomas O'Brien", "Priya Nair",
                "Robert Kim", "Sarah Collins", "Michael Torres", "Linda Chen",
            ];
            let mut rename_idx = 0;
            let mut used_names: std::collections::HashSet<String> = reserved_names.clone();

            for id in &tier3_ids {
                if let Some(agent) = s.agents.get_mut(id) {
                    if used_names.contains(&agent.name.to_lowercase()) {
                        // Name collision — rename this juror
                        while rename_idx < rename_pool.len()
                            && used_names.contains(&rename_pool[rename_idx].to_lowercase())
                        {
                            rename_idx += 1;
                        }
                        if rename_idx < rename_pool.len() {
                            tracing::info!("Renamed juror '{}' -> '{}' (name collision)", agent.name, rename_pool[rename_idx]);
                            agent.name = rename_pool[rename_idx].to_string();
                            agent.username = format!("juror_{}", rename_pool[rename_idx].to_lowercase().replace(' ', "_"));
                            rename_idx += 1;
                        }
                    }
                    used_names.insert(agent.name.to_lowercase());
                }
            }

            let mut seat = 1u8;
            for id in &tier3_ids {
                if seat <= 12 {
                    trial_state.add_juror(*id, seat);
                    seat += 1;
                }
            }

            // If we have fewer than 12 tier3 agents, create synthetic jurors
            // to fill the remaining seats
            let juror_names = [
                "Alex Thompson", "Carmen Rivera", "James Park", "Sofia Andersson",
                "Marcus Johnson", "Elena Petrova", "Daniel Kim", "Olivia Williams",
                "Hassan Ahmed", "Grace Chen", "Thomas O'Brien", "Priya Nair",
            ];
            while seat <= 12 {
                let juror_id = Uuid::new_v4();
                let idx = (seat - 1) as usize;
                let name = juror_names.get(idx).unwrap_or(&"Juror");
                let profile = crate::agent::AgentProfile {
                    id: juror_id,
                    name: name.to_string(),
                    username: format!("juror_{}", seat),
                    tier: Tier::Tier3,
                    bio: format!("Juror #{} in the trial", seat),
                    persona: "An ordinary citizen serving on jury duty.".into(),
                    stance: crate::agent::Stance::Neutral,
                    sentiment_bias: 0.0,
                    influence_weight: 0.1,
                    archetype: crate::agent::BehaviorArchetype::Normie,
                    activity_level: 1.0,
                    active_hours: (0..24).collect(),
                    interests: vec!["justice".into()],
                    age: Some(30 + (seat as u32 * 3)),
                    profession: Some("Citizen".into()),
                    source_entity: None,
                    country: None,
                    language_style: "casual".into(),
                };
                s.agents.insert(juror_id, profile);
                s.agent_states.insert(juror_id, AgentState::new(juror_id));
                trial_state.add_juror(juror_id, seat);
                seat += 1;
            }

            s.trial = Some(trial_state);
            tracing::info!("Trial mode initialized with 12 jurors ({} from extraction, {} synthetic)",
                tier3_ids.len().min(12), 12_usize.saturating_sub(tier3_ids.len()));
        }

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

/// Pre-populate the social graph with realistic structure.
/// - Power-law follower distribution (few hubs, many peripherals)
/// - Community clusters with sparse cross-community bridges
/// - Some isolated/low-connectivity agents (lurkers)
/// - VIPs don't get followed by everyone
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

    let mut all_ids: Vec<Uuid> = state.agents.keys().cloned().collect();
    all_ids.sort_by_key(|id| id.to_string());

    // Create 4-6 communities based on stance/interests alignment
    let n_communities = (all_ids.len() / 8).max(3).min(6);
    let mut communities: Vec<Vec<Uuid>> = (0..n_communities).map(|_| Vec::new()).collect();

    // Assign agents to communities based on stance similarity (not round-robin)
    for &id in &all_ids {
        let profile = match state.agents.get(&id) {
            Some(p) => p,
            None => continue,
        };
        // Community assignment based on stance + a bit of randomness
        let base = match profile.stance {
            crate::agent::Stance::Supportive => 0,
            crate::agent::Stance::Opposing => 1,
            crate::agent::Stance::Neutral => 2,
            crate::agent::Stance::Observer => 3,
        };
        let jitter = (rand::random::<usize>()) % 2; // some randomness
        let community_idx = (base + jitter) % n_communities;
        communities[community_idx].push(id);
    }

    let mut add_follow = |follower: Uuid, target: Uuid, state: &mut SimulationState| {
        if follower == target || state.world.social_graph.is_following(&follower, &target) {
            return;
        }
        state.world.social_graph.add_follow(follower, target);
        if let Some(st) = state.agent_states.get_mut(&follower) {
            st.following.push(target);
        }
        if let Some(st) = state.agent_states.get_mut(&target) {
            st.followers.push(follower);
        }
    };

    // VIP-to-VIP: sparse connections (they don't all know each other)
    for &vip_id in &tier1_ids {
        for &other_vip in &tier1_ids {
            if vip_id != other_vip && rand::random::<f32>() < 0.35 {
                add_follow(vip_id, other_vip, state);
            }
        }
    }

    // VIPs get followed based on power-law: first VIP gets most, last gets least
    // NOT everyone follows every VIP
    for (vip_rank, &vip_id) in tier1_ids.iter().enumerate() {
        // Base probability decreases with rank (first VIP = most popular)
        let base_prob = 0.5 / (1.0 + vip_rank as f32 * 0.3);
        for &agent_id in &all_ids {
            if agent_id == vip_id {
                continue;
            }
            // Agents in same community as VIP follow more
            let same_community = communities.iter().any(|c| c.contains(&vip_id) && c.contains(&agent_id));
            let prob = if same_community { base_prob * 1.3 } else { base_prob * 0.7 };
            if rand::random::<f32>() < prob {
                add_follow(agent_id, vip_id, state);
            }
        }
    }

    // Tier2: follow within community (dense) + sparse cross-community bridges
    for &agent_id in &tier2_ids {
        let community_idx = communities
            .iter()
            .position(|c| c.contains(&agent_id))
            .unwrap_or(0);
        let community = &communities[community_idx];
        let mut rng = rand::thread_rng();

        // Follow 2-4 within community
        let same_comm: Vec<Uuid> = community.iter().copied().filter(|id| *id != agent_id).collect();
        let n_same = 2 + (rand::random::<usize>() % 3);
        for target in same_comm.choose_multiple(&mut rng, n_same.min(same_comm.len())) {
            if rand::random::<f32>() < 0.7 {
                add_follow(agent_id, *target, state);
            }
        }

        // Bridge: follow 0-2 outside community (sparse, creates inter-community links)
        let external: Vec<Uuid> = all_ids.iter().copied()
            .filter(|id| !community.contains(id) && *id != agent_id && !tier1_ids.contains(id))
            .collect();
        let n_ext = rand::random::<usize>() % 3; // 0, 1, or 2
        for target in external.choose_multiple(&mut rng, n_ext.min(external.len())) {
            if rand::random::<f32>() < 0.3 {
                add_follow(agent_id, *target, state);
            }
        }
    }

    // Tier3: realistic power-law following count
    for &agent_id in &all_ids {
        let profile = match state.agents.get(&agent_id) {
            Some(p) => p,
            None => continue,
        };
        if !matches!(profile.tier, Tier::Tier3) {
            continue;
        }

        let community_idx = communities
            .iter()
            .position(|c| c.contains(&agent_id))
            .unwrap_or(0);
        let community = &communities[community_idx];
        let mut rng = rand::thread_rng();

        // Lurkers follow very few, normies follow a moderate amount
        let base_follows = match profile.archetype {
            crate::agent::BehaviorArchetype::Lurker => 1 + (rand::random::<usize>() % 2),
            crate::agent::BehaviorArchetype::Normie => 2 + (rand::random::<usize>() % 3),
            _ => 2 + (rand::random::<usize>() % 4),
        };

        // Follow within community
        let mut targets: Vec<Uuid> = community.iter().copied()
            .filter(|id| *id != agent_id)
            .collect();
        targets.shuffle(&mut rng);
        for target in targets.into_iter().take(base_follows) {
            if rand::random::<f32>() < 0.6 {
                add_follow(agent_id, target, state);
            }
        }

        // Maybe follow 1 VIP (not all!)
        if !tier1_ids.is_empty() && rand::random::<f32>() < 0.35 {
            let vip = tier1_ids[rand::random::<usize>() % tier1_ids.len()];
            add_follow(agent_id, vip, state);
        }
    }

    // Some agents should be completely isolated (5-10% of tier3)
    // They already are if they got unlucky with the random rolls above

    let total_follows: usize = state.world.social_graph.following.values().map(|v| v.len()).sum();
    let isolated = all_ids.iter()
        .filter(|id| state.world.social_graph.following.get(id).map_or(true, |f| f.is_empty()))
        .count();
    tracing::info!(
        "Seeded social graph: {} follow relationships, {} isolated agents, {} communities",
        total_follows, isolated, n_communities
    );
}
