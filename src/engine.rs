use std::collections::HashMap;
use std::sync::Arc;

use chrono::{Duration, Utc};
use rand::Rng;
use tokio::sync::{broadcast, mpsc, RwLock, Semaphore};
use uuid::Uuid;

use crate::agent::{AgentProfile, AgentState, Tier};
use crate::config::SimConfig;
use crate::llm::{self, LlmClient, PostSummary};
use crate::output::{self, ActionLogger};
use crate::world::*;

// ---------------------------------------------------------------------------
// Shared state (engine <-> server)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SimStatus {
    Idle,
    Preparing,
    Running,
    Paused,
    Finished,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SimSnapshot {
    pub status: SimStatus,
    pub current_round: u32,
    pub total_rounds: u32,
    pub total_agents: usize,
    pub total_actions: usize,
    pub total_posts: usize,
}

pub type SharedState = Arc<RwLock<SimulationState>>;

#[derive(Debug)]
pub struct SimulationState {
    pub status: SimStatus,
    pub agents: HashMap<Uuid, AgentProfile>,
    pub agent_states: HashMap<Uuid, AgentState>,
    pub world: WorldState,
    pub config: SimConfig,
    pub total_actions: usize,
}

impl SimulationState {
    pub fn snapshot(&self) -> SimSnapshot {
        SimSnapshot {
            status: self.status.clone(),
            current_round: self.world.current_round,
            total_rounds: self.config.simulation.total_rounds,
            total_agents: self.agents.len(),
            total_actions: self.total_actions,
            total_posts: self.world.posts.len(),
        }
    }
}

// ---------------------------------------------------------------------------
// WebSocket broadcast events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type")]
pub enum WsEvent {
    #[serde(rename = "action")]
    Action { data: Action },
    #[serde(rename = "round_start")]
    RoundStart { round: u32, active_agents: usize },
    #[serde(rename = "round_end")]
    RoundEnd { round: u32, summary: RoundSummary },
    #[serde(rename = "god_eye_inject")]
    GodEyeInject { event: InjectedEvent },
    #[serde(rename = "trending_update")]
    TrendingUpdate { posts: Vec<PostSummary> },
    #[serde(rename = "simulation_end")]
    SimulationEnd { total_rounds: u32, total_actions: usize },
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

pub struct SimulationEngine {
    state: SharedState,
    llm: Arc<LlmClient>,
    ws_tx: broadcast::Sender<WsEvent>,
    god_eye_rx: mpsc::Receiver<InjectedEvent>,
    pause_rx: mpsc::Receiver<()>,
    resume_rx: mpsc::Receiver<()>,
    stop_rx: mpsc::Receiver<()>,
    paused: bool,
}

/// Channels for external control.
pub struct EngineControls {
    pub pause_tx: mpsc::Sender<()>,
    pub resume_tx: mpsc::Sender<()>,
    pub stop_tx: mpsc::Sender<()>,
    pub god_eye_tx: mpsc::Sender<InjectedEvent>,
    pub ws_tx: broadcast::Sender<WsEvent>,
}

impl SimulationEngine {
    pub fn new(
        state: SharedState,
        llm: Arc<LlmClient>,
        god_eye_rx: mpsc::Receiver<InjectedEvent>,
        ws_tx: broadcast::Sender<WsEvent>,
        pause_rx: mpsc::Receiver<()>,
        resume_rx: mpsc::Receiver<()>,
        stop_rx: mpsc::Receiver<()>,
    ) -> Self {
        Self {
            state,
            llm,
            ws_tx,
            god_eye_rx,
            pause_rx,
            resume_rx,
            stop_rx,
            paused: false,
        }
    }

    /// Run the full simulation loop.
    pub async fn run(&mut self, verbose: bool) -> anyhow::Result<()> {
        let config = {
            let s = self.state.read().await;
            s.config.clone()
        };

        let output_dir = &config.output.output_dir;
        let mut logger = ActionLogger::new(output_dir, &config.output.action_log)?;
        let pb = output::create_progress_bar(config.simulation.total_rounds);

        {
            let mut s = self.state.write().await;
            s.status = SimStatus::Running;
        }

        for round in 1..=config.simulation.total_rounds {
            // Check for stop signal
            if self.stop_rx.try_recv().is_ok() {
                tracing::info!("Stop signal received at round {round}");
                break;
            }

            // Handle pause/resume
            if self.pause_rx.try_recv().is_ok() {
                self.paused = true;
                let mut s = self.state.write().await;
                s.status = SimStatus::Paused;
                tracing::info!("Simulation paused at round {round}");
            }
            while self.paused {
                if self.resume_rx.try_recv().is_ok() {
                    self.paused = false;
                    let mut s = self.state.write().await;
                    s.status = SimStatus::Running;
                    tracing::info!("Simulation resumed");
                }
                if self.stop_rx.try_recv().is_ok() {
                    let mut s = self.state.write().await;
                    s.status = SimStatus::Finished;
                    return Ok(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }

            // Execute round
            let summary = self.execute_round(round, &config, &mut logger, verbose).await?;

            // Broadcast round end
            let _ = self.ws_tx.send(WsEvent::RoundEnd {
                round,
                summary: summary.clone(),
            });

            // Update state
            {
                let mut s = self.state.write().await;
                s.world.round_summaries.push(summary);
            }

            logger.flush()?;
            pb.set_position(round as u64);
        }

        pb.finish_with_message("Simulation complete");

        {
            let mut s = self.state.write().await;
            s.status = SimStatus::Finished;
        }

        let total_actions = self.state.read().await.total_actions;
        let _ = self.ws_tx.send(WsEvent::SimulationEnd {
            total_rounds: config.simulation.total_rounds,
            total_actions,
        });

        Ok(())
    }

    async fn execute_round(
        &mut self,
        round: u32,
        config: &SimConfig,
        logger: &mut ActionLogger,
        verbose: bool,
    ) -> anyhow::Result<RoundSummary> {
        let minutes = config.simulation.minutes_per_round as i64;

        // 1. Advance time
        {
            let mut s = self.state.write().await;
            s.world.current_round = round;
            s.world.simulated_time += Duration::minutes(minutes);
        }

        // 2. Process God's Eye events
        let mut injected_count = 0;
        while let Ok(event) = self.god_eye_rx.try_recv() {
            let should_inject = event
                .inject_at_round
                .map_or(true, |target| target <= round);
            if should_inject {
                self.inject_event(&event).await;
                let _ = self.ws_tx.send(WsEvent::GodEyeInject {
                    event: event.clone(),
                });
                injected_count += 1;
            } else {
                // Re-queue for later (put back in state)
                let mut s = self.state.write().await;
                s.world.injected_events.push(event);
            }
        }

        // Check deferred events
        let deferred: Vec<InjectedEvent> = {
            let mut s = self.state.write().await;
            s.world.injected_events.drain(..).collect()
        };
        let mut remaining = Vec::new();
        for event in deferred {
            if !event.processed
                && event.inject_at_round.map_or(false, |target| target <= round)
            {
                self.inject_event(&event).await;
                let _ = self.ws_tx.send(WsEvent::GodEyeInject {
                    event: event.clone(),
                });
                injected_count += 1;
            } else if !event.processed {
                remaining.push(event);
            }
        }
        {
            let mut s = self.state.write().await;
            s.world.injected_events = remaining;
        }

        // 3. Determine active agents
        let (active_tier1, active_tier2, active_tier3) = {
            let s = self.state.read().await;
            let simulated_hour = s.world.simulated_time.format("%H").to_string();
            let hour: u8 = simulated_hour.parse().unwrap_or(12);

            let mut t1 = Vec::new();
            let mut t2 = Vec::new();
            let mut t3 = Vec::new();

            let mut rng = rand::thread_rng();
            for (id, profile) in &s.agents {
                if !profile.active_hours.contains(&hour) {
                    continue;
                }
                let roll: f32 = rng.gen();
                if roll < profile.activity_level {
                    match profile.tier {
                        Tier::Tier1 => t1.push(*id),
                        Tier::Tier2 => t2.push(*id),
                        Tier::Tier3 => t3.push(*id),
                    }
                }
            }

            (t1, t2, t3)
        };

        let total_active = active_tier1.len() + active_tier2.len() + active_tier3.len();

        // Broadcast round start
        let _ = self.ws_tx.send(WsEvent::RoundStart {
            round,
            active_agents: total_active,
        });

        if verbose {
            output::print_round_summary(&RoundSummary {
                round,
                simulated_time: self.state.read().await.world.simulated_time,
                active_agents: total_active,
                total_actions: 0,
                new_posts: 0,
                new_replies: 0,
                new_likes: 0,
                new_reposts: 0,
                new_follows: 0,
                events_injected: injected_count,
            });
        }

        // 4. Execute tiers sequentially, batches concurrently within each tier
        let mut all_actions: Vec<Action> = Vec::new();
        let mut prior_action_descriptions: Vec<String> = Vec::new();

        // Get event descriptions for this round
        let event_descriptions = self.get_event_descriptions_for_round().await;

        // === TIER 1 ===
        if !active_tier1.is_empty() {
            let actions = self
                .execute_tier(
                    Tier::Tier1,
                    &active_tier1,
                    &prior_action_descriptions,
                    &event_descriptions,
                    config,
                    round,
                )
                .await;
            for action in &actions {
                prior_action_descriptions
                    .push(format!("@{}: {}", action.agent_name, describe_action(action)));
                if verbose {
                    output::print_action(action, true);
                }
                logger.log_action(action)?;
                let _ = self
                    .ws_tx
                    .send(WsEvent::Action { data: action.clone() });
            }
            all_actions.extend(actions);
        }

        // === TIER 2 ===
        if !active_tier2.is_empty() {
            let actions = self
                .execute_tier(
                    Tier::Tier2,
                    &active_tier2,
                    &prior_action_descriptions,
                    &event_descriptions,
                    config,
                    round,
                )
                .await;
            for action in &actions {
                prior_action_descriptions
                    .push(format!("@{}: {}", action.agent_name, describe_action(action)));
                if verbose {
                    output::print_action(action, true);
                }
                logger.log_action(action)?;
                let _ = self
                    .ws_tx
                    .send(WsEvent::Action { data: action.clone() });
            }
            all_actions.extend(actions);
        }

        // === TIER 3 ===
        if !active_tier3.is_empty() {
            let actions = self
                .execute_tier(
                    Tier::Tier3,
                    &active_tier3,
                    &prior_action_descriptions,
                    &event_descriptions,
                    config,
                    round,
                )
                .await;
            for action in &actions {
                if verbose {
                    output::print_action(action, true);
                }
                logger.log_action(action)?;
                let _ = self
                    .ws_tx
                    .send(WsEvent::Action { data: action.clone() });
            }
            all_actions.extend(actions);
        }

        // 5. Build round summary
        let mut new_posts = 0;
        let mut new_replies = 0;
        let mut new_likes = 0;
        let mut new_reposts = 0;
        let mut new_follows = 0;

        for action in &all_actions {
            match action.action_type {
                ActionType::CreatePost => new_posts += 1,
                ActionType::Reply => new_replies += 1,
                ActionType::Like => new_likes += 1,
                ActionType::Repost => new_reposts += 1,
                ActionType::Follow => new_follows += 1,
                _ => {}
            }
        }

        // Update total actions count
        {
            let mut s = self.state.write().await;
            s.total_actions += all_actions.len();
        }

        Ok(RoundSummary {
            round,
            simulated_time: self.state.read().await.world.simulated_time,
            active_agents: total_active,
            total_actions: all_actions.len(),
            new_posts,
            new_replies,
            new_likes,
            new_reposts,
            new_follows,
            events_injected: injected_count,
        })
    }

    /// Execute a single tier: create batches, fire concurrently, apply results.
    async fn execute_tier(
        &self,
        tier: Tier,
        agent_ids: &[Uuid],
        prior_actions: &[String],
        events: &[String],
        config: &SimConfig,
        round: u32,
    ) -> Vec<Action> {
        let settings = self.llm.settings_for(tier);
        let batch_size = settings.batch_size;
        let max_concurrency = settings.max_concurrency;

        // Create batches
        let batches: Vec<Vec<Uuid>> = agent_ids.chunks(batch_size).map(|c| c.to_vec()).collect();

        let semaphore = Arc::new(Semaphore::new(max_concurrency));
        let mut handles = Vec::new();

        for batch in batches {
            let sem = semaphore.clone();
            let llm = self.llm.clone();
            let state = self.state.clone();
            let prior = prior_actions.to_vec();
            let evts = events.to_vec();
            let total_rounds = config.simulation.total_rounds;
            let world_config = config.world.clone();

            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                execute_batch(llm, state, tier, &batch, &prior, &evts, round, total_rounds, &world_config).await
            }));
        }

        let mut all_actions = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(actions) => all_actions.extend(actions),
                Err(e) => tracing::error!("Batch task failed: {e}"),
            }
        }

        // Apply actions to world state
        let simulated_time = self.state.read().await.world.simulated_time;
        for action in &mut all_actions {
            action.simulated_time = simulated_time;
            let mut s = self.state.write().await;
            apply_action(&mut s, action);
        }

        all_actions
    }

    async fn inject_event(&self, event: &InjectedEvent) {
        let mut s = self.state.write().await;
        match event.event_type {
            InjectedEventType::BreakingNews | InjectedEventType::ViralContent => {
                let post = Post {
                    id: Uuid::new_v4(),
                    author_id: Uuid::nil(),
                    author_name: "[SYSTEM]".into(),
                    content: event.content.clone(),
                    created_at_round: s.world.current_round,
                    simulated_time: s.world.simulated_time,
                    reply_to: None,
                    repost_of: None,
                    likes: Vec::new(),
                    replies: Vec::new(),
                    reposts: Vec::new(),
                    hashtags: Vec::new(),
                };
                s.world.add_post(post);
            }
            InjectedEventType::AgentMood => {
                // Parse "agent:username sentiment_bias:X"
                if let Some((agent_part, bias_part)) = event.content.split_once(" sentiment_bias:") {
                    let username = agent_part.trim_start_matches("agent:");
                    if let Ok(bias) = bias_part.parse::<f32>() {
                        for profile in s.agents.values_mut() {
                            if profile.username == username {
                                profile.sentiment_bias = bias;
                                break;
                            }
                        }
                    }
                }
            }
            InjectedEventType::SystemAnnouncement => {
                let post = Post {
                    id: Uuid::new_v4(),
                    author_id: Uuid::nil(),
                    author_name: "[ANNOUNCEMENT]".into(),
                    content: event.content.clone(),
                    created_at_round: s.world.current_round,
                    simulated_time: s.world.simulated_time,
                    reply_to: None,
                    repost_of: None,
                    likes: Vec::new(),
                    replies: Vec::new(),
                    reposts: Vec::new(),
                    hashtags: Vec::new(),
                };
                s.world.add_post(post);
            }
            _ => {}
        }
        tracing::info!("God's Eye: injected event '{}' ({:?})", event.id, event.event_type);
    }

    async fn get_event_descriptions_for_round(&self) -> Vec<String> {
        let s = self.state.read().await;
        s.world
            .posts
            .values()
            .filter(|p| {
                p.created_at_round == s.world.current_round
                    && (p.author_name == "[SYSTEM]" || p.author_name == "[ANNOUNCEMENT]")
            })
            .map(|p| format!("[{}]: {}", p.author_name, p.content))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Batch execution (spawned as tokio task)
// ---------------------------------------------------------------------------

async fn execute_batch(
    llm: Arc<LlmClient>,
    state: SharedState,
    tier: Tier,
    agent_ids: &[Uuid],
    prior_actions: &[String],
    events: &[String],
    round: u32,
    total_rounds: u32,
    world_config: &crate::config::WorldConfig,
) -> Vec<Action> {
    let s = state.read().await;
    let simulated_time = s.world.simulated_time.format("%Y-%m-%d %H:%M").to_string();

    if agent_ids.len() == 1 {
        // Individual call
        let agent_id = agent_ids[0];
        let Some(profile) = s.agents.get(&agent_id).cloned() else {
            return Vec::new();
        };
        let memory_text = s
            .agent_states
            .get(&agent_id)
            .map(|st| st.memory.render(round))
            .unwrap_or_default();

        let feed = s.world.build_feed(
            &agent_id,
            world_config.feed_size,
            world_config.recency_weight,
            world_config.popularity_weight,
            world_config.relevance_weight,
        );
        let feed_summaries: Vec<PostSummary> = feed
            .iter()
            .map(|p| PostSummary::from_post(p, round, 120))
            .collect();

        let trending = s.world.trending(world_config.trending_count, 10);
        let trending_summaries: Vec<PostSummary> = trending
            .iter()
            .map(|p| PostSummary::from_post(p, round, 80))
            .collect();

        let system = llm::build_single_system_prompt(&profile);
        let user = llm::build_single_user_prompt(
            round,
            total_rounds,
            &simulated_time,
            &memory_text,
            &feed_summaries,
            &trending_summaries,
            events,
        );

        drop(s); // Release read lock before LLM call

        match llm.call_tier(tier, &system, &user).await {
            Ok(raw) => {
                if let Some(parsed) = llm::parse_single_response(&raw) {
                    convert_single_actions(agent_id, &profile, &parsed, round, events)
                } else {
                    tracing::warn!("Failed to parse response for @{}", profile.username);
                    Vec::new()
                }
            }
            Err(e) => {
                tracing::error!("LLM call failed for @{}: {e}", profile.username);
                Vec::new()
            }
        }
    } else {
        // Batch call
        let persona_max = match tier {
            Tier::Tier2 => 200,
            Tier::Tier3 => 100,
            _ => 500,
        };
        let memory_recent_count = match tier {
            Tier::Tier2 => 5,
            Tier::Tier3 => 3,
            _ => 10,
        };

        let agents_with_memory: Vec<(AgentProfile, String)> = agent_ids
            .iter()
            .filter_map(|id| {
                let profile = s.agents.get(id)?.clone();
                let memory = s
                    .agent_states
                    .get(id)
                    .map(|st| st.memory.render_short(round, memory_recent_count))
                    .unwrap_or_default();
                Some((profile, memory))
            })
            .collect();

        // Build shared feed (use first agent's feed as representative)
        let first_id = agent_ids[0];
        let feed = s.world.build_feed(
            &first_id,
            world_config.feed_size,
            world_config.recency_weight,
            world_config.popularity_weight,
            world_config.relevance_weight,
        );
        let feed_summaries: Vec<PostSummary> = feed
            .iter()
            .take(10)
            .map(|p| PostSummary::from_post(p, round, 80))
            .collect();

        let trending = s.world.trending(5, 10);
        let trending_summaries: Vec<PostSummary> = trending
            .iter()
            .map(|p| PostSummary::from_post(p, round, 60))
            .collect();

        let system = llm::build_batch_system_prompt(&agents_with_memory, persona_max);
        let user = llm::build_batch_user_prompt(
            round,
            total_rounds,
            &simulated_time,
            &feed_summaries,
            &trending_summaries,
            prior_actions,
            events,
        );

        // Clone profiles for post-parse use
        let profiles: HashMap<String, (Uuid, AgentProfile)> = agents_with_memory
            .iter()
            .map(|(p, _)| (p.id.to_string()[..8].to_string(), (p.id, p.clone())))
            .collect();

        drop(s);

        match llm.call_tier(tier, &system, &user).await {
            Ok(raw) => {
                if let Some(parsed) = llm::parse_batch_response(&raw) {
                    convert_batch_actions(&profiles, &parsed, round)
                } else {
                    tracing::warn!("Failed to parse batch response for {} agents", agent_ids.len());
                    Vec::new()
                }
            }
            Err(e) => {
                tracing::error!("Batch LLM call failed ({} agents): {e}", agent_ids.len());
                Vec::new()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Action conversion
// ---------------------------------------------------------------------------

fn convert_single_actions(
    agent_id: Uuid,
    profile: &AgentProfile,
    parsed: &llm::SingleAgentResponse,
    round: u32,
    _events: &[String],
) -> Vec<Action> {
    let mut actions = Vec::new();

    for pa in &parsed.actions {
        let action_type = match pa.action_type.to_lowercase().as_str() {
            "create_post" => ActionType::CreatePost,
            "reply" => ActionType::Reply,
            "like" => ActionType::Like,
            "repost" => ActionType::Repost,
            "follow" => ActionType::Follow,
            "unfollow" => ActionType::Unfollow,
            "do_nothing" => ActionType::DoNothing,
            "pin_memory" => ActionType::PinMemory,
            _ => continue,
        };

        actions.push(Action {
            id: Uuid::new_v4(),
            round,
            simulated_time: Utc::now(), // Will be overwritten
            agent_id,
            agent_name: profile.username.clone(),
            agent_tier: profile.tier,
            action_type,
            content: pa.content.clone(),
            target_post_id: pa.target_post_id.as_ref().and_then(|s| {
                Uuid::parse_str(s).map_err(|e| {
                    tracing::debug!("Invalid target_post_id '{}' from @{}: {e}", s, profile.username);
                    e
                }).ok()
            }),
            target_agent_id: pa.target_agent_id.as_ref().and_then(|s| {
                Uuid::parse_str(s).map_err(|e| {
                    tracing::debug!("Invalid target_agent_id '{}' from @{}: {e}", s, profile.username);
                    e
                }).ok()
            }),
            reasoning: parsed.reasoning.clone(),
        });
    }

    // Handle pin_memory
    if let Some(pin) = &parsed.pin_memory {
        actions.push(Action {
            id: Uuid::new_v4(),
            round,
            simulated_time: Utc::now(),
            agent_id,
            agent_name: profile.username.clone(),
            agent_tier: profile.tier,
            action_type: ActionType::PinMemory,
            content: Some(pin.clone()),
            target_post_id: None,
            target_agent_id: None,
            reasoning: None,
        });
    }

    actions
}

fn convert_batch_actions(
    profiles: &HashMap<String, (Uuid, AgentProfile)>,
    parsed: &llm::BatchAgentResponse,
    round: u32,
) -> Vec<Action> {
    let mut actions = Vec::new();

    for entry in &parsed.agent_actions {
        let Some((agent_id, profile)) = profiles.get(&entry.agent_id) else {
            continue;
        };

        for pa in &entry.actions {
            let action_type = match pa.action_type.to_lowercase().as_str() {
                "create_post" => ActionType::CreatePost,
                "reply" => ActionType::Reply,
                "like" => ActionType::Like,
                "repost" => ActionType::Repost,
                "follow" => ActionType::Follow,
                "unfollow" => ActionType::Unfollow,
                "do_nothing" => ActionType::DoNothing,
                "pin_memory" => ActionType::PinMemory,
                _ => continue,
            };

            actions.push(Action {
                id: Uuid::new_v4(),
                round,
                simulated_time: Utc::now(),
                agent_id: *agent_id,
                agent_name: profile.username.clone(),
                agent_tier: profile.tier,
                action_type,
                content: pa.content.clone(),
                target_post_id: pa.target_post_id.as_ref().and_then(|s| {
                    Uuid::parse_str(s).map_err(|e| {
                        tracing::debug!("Invalid target_post_id '{}' from batch @{}: {e}", s, profile.username);
                        e
                    }).ok()
                }),
                target_agent_id: pa.target_agent_id.as_ref().and_then(|s| {
                    Uuid::parse_str(s).map_err(|e| {
                        tracing::debug!("Invalid target_agent_id '{}' from batch @{}: {e}", s, profile.username);
                        e
                    }).ok()
                }),
                reasoning: entry.reasoning.clone(),
            });
        }

        if let Some(pin) = &entry.pin_memory {
            actions.push(Action {
                id: Uuid::new_v4(),
                round,
                simulated_time: Utc::now(),
                agent_id: *agent_id,
                agent_name: profile.username.clone(),
                agent_tier: profile.tier,
                action_type: ActionType::PinMemory,
                content: Some(pin.clone()),
                target_post_id: None,
                target_agent_id: None,
                reasoning: None,
            });
        }
    }

    actions
}

// ---------------------------------------------------------------------------
// Apply action to world state
// ---------------------------------------------------------------------------

fn apply_action(state: &mut SimulationState, action: &Action) {
    match &action.action_type {
        ActionType::CreatePost => {
            if let Some(content) = &action.content {
                let post = Post {
                    id: action.id,
                    author_id: action.agent_id,
                    author_name: action.agent_name.clone(),
                    content: content.clone(),
                    created_at_round: action.round,
                    simulated_time: action.simulated_time,
                    reply_to: None,
                    repost_of: None,
                    likes: Vec::new(),
                    replies: Vec::new(),
                    reposts: Vec::new(),
                    hashtags: extract_hashtags(content),
                };
                state.world.add_post(post);
                if let Some(agent_state) = state.agent_states.get_mut(&action.agent_id) {
                    agent_state.post_ids.push(action.id);
                }
            }
        }
        ActionType::Reply => {
            if let (Some(content), Some(target)) = (&action.content, &action.target_post_id) {
                let post = Post {
                    id: action.id,
                    author_id: action.agent_id,
                    author_name: action.agent_name.clone(),
                    content: content.clone(),
                    created_at_round: action.round,
                    simulated_time: action.simulated_time,
                    reply_to: Some(*target),
                    repost_of: None,
                    likes: Vec::new(),
                    replies: Vec::new(),
                    reposts: Vec::new(),
                    hashtags: Vec::new(),
                };
                state.world.add_post(post);
            }
        }
        ActionType::Like => {
            if let Some(target) = action.target_post_id {
                state.world.add_like(target, action.agent_id);
                if let Some(agent_state) = state.agent_states.get_mut(&action.agent_id) {
                    agent_state.liked_post_ids.push(target);
                }
            }
        }
        ActionType::Repost => {
            if let Some(target) = action.target_post_id {
                let repost = Post {
                    id: action.id,
                    author_id: action.agent_id,
                    author_name: action.agent_name.clone(),
                    content: action.content.clone().unwrap_or_default(),
                    created_at_round: action.round,
                    simulated_time: action.simulated_time,
                    reply_to: None,
                    repost_of: Some(target),
                    likes: Vec::new(),
                    replies: Vec::new(),
                    reposts: Vec::new(),
                    hashtags: Vec::new(),
                };
                state.world.add_repost(target, repost);
            }
        }
        ActionType::Follow => {
            if let Some(target) = action.target_agent_id {
                state.world.social_graph.add_follow(action.agent_id, target);
                if let Some(agent_state) = state.agent_states.get_mut(&action.agent_id) {
                    if !agent_state.following.contains(&target) {
                        agent_state.following.push(target);
                    }
                }
                if let Some(target_state) = state.agent_states.get_mut(&target) {
                    if !target_state.followers.contains(&action.agent_id) {
                        target_state.followers.push(action.agent_id);
                    }
                }
            }
        }
        ActionType::Unfollow => {
            if let Some(target) = action.target_agent_id {
                state
                    .world
                    .social_graph
                    .remove_follow(action.agent_id, target);
            }
        }
        ActionType::PinMemory => {
            if let Some(content) = &action.content {
                if let Some(agent_state) = state.agent_states.get_mut(&action.agent_id) {
                    agent_state.memory.pin(content.clone());
                }
            }
        }
        ActionType::DoNothing => {}
    }

    // Update agent memory with observation
    if !matches!(action.action_type, ActionType::DoNothing | ActionType::PinMemory) {
        if let Some(agent_state) = state.agent_states.get_mut(&action.agent_id) {
            agent_state
                .memory
                .observe(action.round, describe_action(action));
        }
    }
}

fn describe_action(action: &Action) -> String {
    match &action.action_type {
        ActionType::CreatePost => {
            let preview = action
                .content
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(60)
                .collect::<String>();
            format!("posted: \"{preview}\"")
        }
        ActionType::Reply => format!("replied to a post"),
        ActionType::Like => format!("liked a post"),
        ActionType::Repost => format!("reposted"),
        ActionType::Follow => format!("followed someone"),
        ActionType::Unfollow => format!("unfollowed someone"),
        ActionType::DoNothing => format!("idle"),
        ActionType::PinMemory => format!("pinned a memory"),
    }
}

fn extract_hashtags(content: &str) -> Vec<String> {
    content
        .split_whitespace()
        .filter(|w| w.starts_with('#') && w.len() > 1)
        .map(|w| w.to_string())
        .collect()
}
