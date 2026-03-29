use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{Duration, Utc};
use rand::Rng;
use tokio::sync::{broadcast, mpsc, RwLock, Semaphore};
use uuid::Uuid;

use crate::agent::{ActionLogEntry, AgentProfile, AgentState, BehaviorArchetype, Stance, Tier};
use crate::config::SimConfig;
use crate::llm::{self, LlmClient, PostSummary};
use crate::output::{self, ActionLogger};
use crate::trial::{TrialState, TrialSchedule, TrialPhase, CourtRole, JurorState, Party};
use crate::world::*;

use regex_lite::Regex;

// ---------------------------------------------------------------------------
// Shared state (engine <-> server)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    pub scenario_prompt: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub estimated_cost: f64,
}

pub type SharedState = Arc<RwLock<SimulationState>>;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SimulationState {
    pub status: SimStatus,
    pub agents: HashMap<Uuid, AgentProfile>,
    pub agent_states: HashMap<Uuid, AgentState>,
    pub world: WorldState,
    pub config: SimConfig,
    pub total_actions: usize,
    pub syntheses: Vec<(u32, String)>,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    /// Trial-mode state (None for standard simulations).
    #[serde(default)]
    pub trial: Option<TrialState>,
}

impl SimulationState {
    pub fn snapshot(&self) -> SimSnapshot {
        // Sum up cost across all tiers
        let cost = self.estimate_cost();
        SimSnapshot {
            status: self.status.clone(),
            current_round: self.world.current_round,
            total_rounds: self.config.simulation.total_rounds,
            total_agents: self.agents.len(),
            total_actions: self.total_actions,
            total_posts: self.world.posts.len(),
            scenario_prompt: self.config.simulation.scenario_prompt.clone(),
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            estimated_cost: cost,
        }
    }

    /// Save state to a JSON file.
    pub fn save_to_file(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        tracing::info!("State saved to {}", path.display());
        Ok(())
    }

    /// Load state from a JSON file.
    pub fn load_from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        let mut state: Self = serde_json::from_str(&json)?;
        state.status = SimStatus::Finished; // Always start as finished after load
        tracing::info!("State loaded from {}", path.display());
        Ok(state)
    }

    fn estimate_cost(&self) -> f64 {
        // Use tier1 pricing as representative (all tiers use same model in typical config)
        let input_price = self.config.tiers.tier1.input_price_per_mtok;
        let output_price = self.config.tiers.tier1.output_price_per_mtok;
        if input_price == 0.0 && output_price == 0.0 {
            // Default Gemini 2.0 Flash pricing: $0.10/1M in, $0.40/1M out
            (self.prompt_tokens as f64 * 0.10 + self.completion_tokens as f64 * 0.40) / 1_000_000.0
        } else {
            (self.prompt_tokens as f64 * input_price + self.completion_tokens as f64 * output_price) / 1_000_000.0
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
    RoundEnd {
        round: u32,
        summary: RoundSummary,
        prompt_tokens: u64,
        completion_tokens: u64,
        estimated_cost: f64,
    },
    #[serde(rename = "god_eye_inject")]
    GodEyeInject { event: InjectedEvent },
    #[serde(rename = "trending_update")]
    TrendingUpdate { posts: Vec<PostSummary> },
    #[serde(rename = "synthesis")]
    Synthesis { round: u32, text: String },
    #[serde(rename = "simulation_end")]
    SimulationEnd { total_rounds: u32, total_actions: usize },
    #[serde(rename = "status_update")]
    StatusUpdate {
        status: String,
        current_round: u32,
        total_rounds: u32,
        total_agents: usize,
        total_actions: usize,
        total_posts: usize,
        scenario_prompt: String,
        prompt_tokens: u64,
        completion_tokens: u64,
    },
    // Trial-specific events
    #[serde(rename = "trial_argument")]
    TrialArgument {
        round: u32,
        speaker_id: String,
        speaker_name: String,
        speaker_role: String,
        content: String,
        jury_impact: Vec<(u8, f32, f32)>, // (seat, delta, new_conviction)
    },
    #[serde(rename = "trial_jury_update")]
    TrialJuryUpdate {
        round: u32,
        jurors: Vec<TrialJurorSnapshot>,
    },
    #[serde(rename = "trial_phase_change")]
    TrialPhaseChange {
        phase: String,
        round: u32,
    },
    #[serde(rename = "trial_objection")]
    TrialObjection {
        round: u32,
        by_name: String,
        grounds: String,
        ruling: String,
    },
    #[serde(rename = "trial_verdict")]
    TrialVerdict {
        result: String,
        guilty: usize,
        not_guilty: usize,
        unanimous: bool,
    },
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TrialJurorSnapshot {
    pub seat: u8,
    pub name: String,
    pub conviction: f32,
    pub confidence: f32,
    pub conviction_label: String,
    pub trust_prosecution: f32,
    pub trust_defense: f32,
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
            // Broadcast status update so clients get the new scenario_prompt
            let snap = s.snapshot();
            let _ = self.ws_tx.send(WsEvent::StatusUpdate {
                status: format!("{:?}", snap.status).to_lowercase(),
                current_round: snap.current_round,
                total_rounds: snap.total_rounds,
                total_agents: snap.total_agents,
                total_actions: snap.total_actions,
                total_posts: snap.total_posts,
                scenario_prompt: snap.scenario_prompt,
                prompt_tokens: snap.prompt_tokens,
                completion_tokens: snap.completion_tokens,
            });
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

            // Update token counts from LLM client
            let (pt, ct) = self.llm.token_usage();

            // Calculate cost
            let cost = {
                let s = self.state.read().await;
                let input_price = s.config.tiers.tier1.input_price_per_mtok;
                let output_price = s.config.tiers.tier1.output_price_per_mtok;
                if input_price == 0.0 && output_price == 0.0 {
                    (pt as f64 * 0.10 + ct as f64 * 0.40) / 1_000_000.0
                } else {
                    (pt as f64 * input_price + ct as f64 * output_price) / 1_000_000.0
                }
            };

            // Broadcast round end
            let _ = self.ws_tx.send(WsEvent::RoundEnd {
                round,
                summary: summary.clone(),
                prompt_tokens: pt,
                completion_tokens: ct,
                estimated_cost: cost,
            });

            // Update state
            {
                let mut s = self.state.write().await;
                s.world.round_summaries.push(summary);
                s.prompt_tokens = pt;
                s.completion_tokens = ct;
            }

            // Fire webhook if configured
            fire_webhook_if_needed(&config, "round_end", &serde_json::json!({"round": round}));

            // Generate synthesis if needed
            if config.synthesis.enabled
                && round > 0
                && round % config.synthesis.every_n_rounds == 0
            {
                if let Some(text) = self.generate_synthesis(round, &config).await {
                    let _ = self.ws_tx.send(WsEvent::Synthesis {
                        round,
                        text: text.clone(),
                    });
                    let mut s = self.state.write().await;
                    s.syntheses.push((round, text));
                }
            }

            logger.flush()?;
            pb.set_position(round as u64);
        }

        pb.finish_with_message("Simulation complete");
        fire_webhook_if_needed(&config, "simulation_end", &serde_json::json!({"total_rounds": config.simulation.total_rounds}));

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

    /// Continue a finished simulation for additional rounds, keeping all existing state.
    pub async fn run_continuation(&mut self, extra_rounds: u32, verbose: bool) -> anyhow::Result<()> {
        let config = {
            let s = self.state.read().await;
            s.config.clone()
        };

        let start_round = {
            let s = self.state.read().await;
            s.world.current_round + 1
        };
        let end_round = start_round + extra_rounds - 1;
        let new_total = end_round;

        // Update total_rounds in config so the UI shows the right denominator
        {
            let mut s = self.state.write().await;
            s.config.simulation.total_rounds = new_total;
        }

        let output_dir = &config.output.output_dir;
        let mut logger = ActionLogger::new(output_dir, &config.output.action_log)?;
        let pb = output::create_progress_bar(extra_rounds);

        {
            let mut s = self.state.write().await;
            s.status = SimStatus::Running;
            let snap = s.snapshot();
            let _ = self.ws_tx.send(WsEvent::StatusUpdate {
                status: format!("{:?}", snap.status).to_lowercase(),
                current_round: snap.current_round,
                total_rounds: snap.total_rounds,
                total_agents: snap.total_agents,
                total_actions: snap.total_actions,
                total_posts: snap.total_posts,
                scenario_prompt: snap.scenario_prompt,
                prompt_tokens: snap.prompt_tokens,
                completion_tokens: snap.completion_tokens,
            });
        }

        for round in start_round..=end_round {
            if self.stop_rx.try_recv().is_ok() {
                tracing::info!("Stop signal received at round {round}");
                break;
            }

            if self.pause_rx.try_recv().is_ok() {
                self.paused = true;
                let mut s = self.state.write().await;
                s.status = SimStatus::Paused;
            }
            while self.paused {
                if self.resume_rx.try_recv().is_ok() {
                    self.paused = false;
                    let mut s = self.state.write().await;
                    s.status = SimStatus::Running;
                }
                if self.stop_rx.try_recv().is_ok() {
                    let mut s = self.state.write().await;
                    s.status = SimStatus::Finished;
                    return Ok(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }

            let summary = self.execute_round(round, &config, &mut logger, verbose).await?;

            let (pt, ct) = self.llm.token_usage();
            let cost = {
                let s = self.state.read().await;
                let input_price = s.config.tiers.tier1.input_price_per_mtok;
                let output_price = s.config.tiers.tier1.output_price_per_mtok;
                if input_price == 0.0 && output_price == 0.0 {
                    (pt as f64 * 0.10 + ct as f64 * 0.40) / 1_000_000.0
                } else {
                    (pt as f64 * input_price + ct as f64 * output_price) / 1_000_000.0
                }
            };
            let _ = self.ws_tx.send(WsEvent::RoundEnd {
                round,
                summary: summary.clone(),
                prompt_tokens: pt,
                completion_tokens: ct,
                estimated_cost: cost,
            });

            {
                let mut s = self.state.write().await;
                s.world.round_summaries.push(summary);
                s.prompt_tokens = pt;
                s.completion_tokens = ct;
            }

            if config.synthesis.enabled
                && round > 0
                && round % config.synthesis.every_n_rounds == 0
            {
                if let Some(text) = self.generate_synthesis(round, &config).await {
                    let _ = self.ws_tx.send(WsEvent::Synthesis {
                        round,
                        text: text.clone(),
                    });
                    let mut s = self.state.write().await;
                    s.syntheses.push((round, text));
                }
            }

            logger.flush()?;
            pb.set_position((round - start_round + 1) as u64);
        }

        pb.finish_with_message("Continuation complete");

        {
            let mut s = self.state.write().await;
            s.status = SimStatus::Finished;
        }

        let total_actions = self.state.read().await.total_actions;
        let _ = self.ws_tx.send(WsEvent::SimulationEnd {
            total_rounds: end_round,
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

        // Trial mode: delegate to trial engine
        {
            let is_trial = self.state.read().await.trial.is_some();
            if is_trial {
                return crate::trial_engine::execute_trial_round(
                    self.state.clone(),
                    self.llm.clone(),
                    &self.ws_tx,
                    round,
                ).await;
            }
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
                new_quote_reposts: 0,
                new_follows: 0,
                events_injected: injected_count,
                new_solutions: 0,
            });
        }

        // 4. Execute tiers SEQUENTIALLY to preserve causal chains (T1 → T2 → T3)
        let mut event_descriptions = self.get_event_descriptions_for_round().await;
        let mut all_actions: Vec<Action> = Vec::new();

        // Devil's advocate round: every 5th round when challenge is active, inject special directive to T1
        let is_devil_advocate = config.simulation.challenge_question.is_some()
            && round > 1
            && round % 5 == 0;
        if is_devil_advocate {
            // Get top solutions by votes/engagement
            let top_solutions = {
                let s = self.state.read().await;
                let mut sols: Vec<(String, String, usize)> = s.world.solution_ids.iter()
                    .filter_map(|id| {
                        let p = s.world.posts.get(id)?;
                        let votes = s.world.solution_votes.get(id).map(|v| v.len()).unwrap_or(0);
                        let content: String = p.content.chars().take(120).collect();
                        Some((p.author_name.clone(), content, votes))
                    })
                    .collect();
                sols.sort_by(|a, b| b.2.cmp(&a.2));
                sols.truncate(3);
                sols
            };

            if !top_solutions.is_empty() {
                let sol_list: String = top_solutions.iter().enumerate()
                    .map(|(i, (author, content, votes))| format!("{}. @{} ({} votes): \"{}\"", i + 1, author, votes, content))
                    .collect::<Vec<_>>()
                    .join("\n");
                event_descriptions.push(format!(
                    "[DEVIL'S ADVOCATE ROUND] This is a special challenge round. Your job is to find WEAKNESSES and COUNTER-ARGUMENTS to the top solutions. Be constructive but critical. Push back on assumptions. Top solutions so far:\n{}",
                    sol_list
                ));
                tracing::info!("Round {round}: Devil's advocate activated with {} solutions", top_solutions.len());
            }
        }

        // Tier 1 — VIPs set the narrative
        let t1_actions = if !active_tier1.is_empty() {
            self.execute_tier(Tier::Tier1, &active_tier1, &[], &event_descriptions, config, round).await
        } else {
            Vec::new()
        };
        for action in &t1_actions {
            if verbose { output::print_action(action, true); }
            logger.log_action(action)?;
            let _ = self.ws_tx.send(WsEvent::Action { data: action.clone() });
        }
        let t1_prior: Vec<String> = t1_actions.iter()
            .filter(|a| !matches!(a.action_type, ActionType::DoNothing | ActionType::PinMemory))
            .map(|a| describe_action_for_context(a))
            .collect();
        all_actions.extend(t1_actions);

        // Tier 2 — Standard agents react to VIPs
        let t2_actions = if !active_tier2.is_empty() {
            self.execute_tier(Tier::Tier2, &active_tier2, &t1_prior, &event_descriptions, config, round).await
        } else {
            Vec::new()
        };
        for action in &t2_actions {
            if verbose { output::print_action(action, true); }
            logger.log_action(action)?;
            let _ = self.ws_tx.send(WsEvent::Action { data: action.clone() });
        }
        let mut t12_prior = t1_prior;
        t12_prior.extend(t2_actions.iter()
            .filter(|a| !matches!(a.action_type, ActionType::DoNothing | ActionType::PinMemory))
            .take(10)
            .map(|a| describe_action_for_context(a)));
        all_actions.extend(t2_actions);

        // Tier 3 — Figurants react to everyone
        let t3_actions = if !active_tier3.is_empty() {
            self.execute_tier(Tier::Tier3, &active_tier3, &t12_prior, &event_descriptions, config, round).await
        } else {
            Vec::new()
        };
        for action in &t3_actions {
            if verbose { output::print_action(action, true); }
            logger.log_action(action)?;
            let _ = self.ws_tx.send(WsEvent::Action { data: action.clone() });
        }
        all_actions.extend(t3_actions);

        // 5. Build engagement notifications for next round
        build_notifications(&self.state, &all_actions, round).await;

        // 6. Update sentiment drift
        update_sentiments(&self.state, round).await;

        // 6b. Update cognitive state and relational memory
        update_cognitive_and_relations(&self.state, &all_actions, round).await;

        // 6c. Update cascade tracking and contested status
        {
            let mut s = self.state.write().await;
            s.world.update_cascades(round);
        }
        update_beliefs_and_contested(&self.state, &all_actions, round).await;

        // 6d. Validate state consistency (debug mode)
        if verbose {
            let s = self.state.read().await;
            let issues = validate_state(&s);
            if !issues.is_empty() {
                tracing::warn!("Round {round}: {} state validation issues", issues.len());
            }
        }

        // 7. Build round summary
        let mut new_posts = 0;
        let mut new_replies = 0;
        let mut new_likes = 0;
        let mut new_reposts = 0;
        let mut new_quote_reposts = 0;
        let mut new_follows = 0;
        let mut new_solutions = 0;

        for action in &all_actions {
            match action.action_type {
                ActionType::CreatePost => new_posts += 1,
                ActionType::Reply => new_replies += 1,
                ActionType::Like => new_likes += 1,
                ActionType::Repost => new_reposts += 1,
                ActionType::QuoteRepost => new_quote_reposts += 1,
                ActionType::Follow => new_follows += 1,
                ActionType::ProposeSolution | ActionType::RefineSolution => new_solutions += 1,
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
            new_quote_reposts,
            new_follows,
            events_injected: injected_count,
            new_solutions,
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
            let challenge_q = config.simulation.challenge_question.clone();

            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                execute_batch(llm, state, tier, &batch, &prior, &evts, round, total_rounds, &world_config, challenge_q).await
            }));
        }

        let mut all_actions = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(actions) => all_actions.extend(actions),
                Err(e) => tracing::error!("Batch task failed: {e}"),
            }
        }

        // Dedup near-duplicate posts within this tier
        dedup_actions(&mut all_actions);

        // Apply actions to world state
        let simulated_time = self.state.read().await.world.simulated_time;
        for action in &mut all_actions {
            action.simulated_time = simulated_time;
            let mut s = self.state.write().await;
            apply_action(&mut s, action);
        }

        all_actions
    }

    async fn generate_synthesis(&self, round: u32, config: &SimConfig) -> Option<String> {
        let s = self.state.read().await;
        let scenario = &config.simulation.scenario_prompt;
        let total_actions = s.total_actions;
        let total_posts = s.world.posts.len();
        let agent_count = s.agents.len();

        let trending = s.world.trending(5, 10);
        let trending_text: Vec<String> = trending
            .iter()
            .map(|p| {
                format!(
                    "- @{}: \"{}\" ({} likes, {} replies)",
                    p.author_name,
                    p.content.chars().take(100).collect::<String>(),
                    p.likes.len(),
                    p.replies.len()
                )
            })
            .collect();

        let recent_summaries: Vec<String> = s
            .world
            .round_summaries
            .iter()
            .rev()
            .take(3)
            .rev()
            .map(|rs| {
                format!(
                    "R{}: {} agents, {} posts, {} replies, {} likes",
                    rs.round, rs.active_agents, rs.new_posts, rs.new_replies, rs.new_likes
                )
            })
            .collect();

        let mut stances: HashMap<String, u32> = HashMap::new();
        for agent in s.agents.values() {
            *stances.entry(agent.stance.to_string()).or_insert(0) += 1;
        }
        let stance_text: Vec<String> = stances.iter().map(|(k, v)| format!("{k}: {v}")).collect();

        drop(s);

        let system = "You are a concise analyst summarizing a social media simulation. Write 2-3 short paragraphs analyzing current dynamics, emerging narratives, and notable trends. Be specific about agent behaviors and sentiment shifts. No markdown headers.".to_string();

        let user = format!(
            "Scenario: {scenario}\n\n\
             Round {round}/{total_rounds}\n\
             Agents: {agent_count} | Posts: {total_posts} | Total actions: {total_actions}\n\n\
             Stance distribution: {stances}\n\n\
             Recent activity:\n{recent}\n\n\
             Trending posts:\n{trending}\n\n\
             Provide a brief narrative analysis of the simulation so far.",
            total_rounds = config.simulation.total_rounds,
            stances = stance_text.join(", "),
            recent = recent_summaries.join("\n"),
            trending = trending_text.join("\n"),
        );

        match self.llm.call_tier(Tier::Tier2, &system, &user).await {
            Ok(text) => {
                let clean = text.trim().to_string();
                if clean.is_empty() {
                    None
                } else {
                    tracing::info!("Synthesis generated for round {round}");
                    Some(clean)
                }
            }
            Err(e) => {
                tracing::warn!("Synthesis generation failed: {e}");
                None
            }
        }
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
                    quote_of: None,
                    refines: None,
                    likes: Vec::new(),
                    replies: Vec::new(),
                    reposts: Vec::new(),
                    hashtags: Vec::new(),
                    cascade_depth: 0,
                    cascade_root: None,
                    contested: false,
                    opposing_reply_count: 0,
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
                    quote_of: None,
                    refines: None,
                    likes: Vec::new(),
                    replies: Vec::new(),
                    reposts: Vec::new(),
                    hashtags: Vec::new(),
                    cascade_depth: 0,
                    cascade_root: None,
                    contested: false,
                    opposing_reply_count: 0,
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
    challenge_question: Option<String>,
) -> Vec<Action> {
    let s = state.read().await;
    let simulated_time = s.world.simulated_time.format("%Y-%m-%d %H:%M").to_string();

    if agent_ids.len() == 1 {
        // Individual call
        let agent_id = agent_ids[0];
        let Some(profile) = s.agents.get(&agent_id).cloned() else {
            return Vec::new();
        };
        let agent_state = s.agent_states.get(&agent_id);
        let memory_text = agent_state
            .map(|st| st.memory.render(round))
            .unwrap_or_default();
        let current_sentiment = agent_state
            .map(|st| st.current_sentiment)
            .unwrap_or(profile.sentiment_bias);

        // Use cognitive state to limit feed size + relational memory for feed bias
        let effective_feed = agent_state
            .map(|st| st.cognitive.effective_feed_size(world_config.feed_size))
            .unwrap_or(world_config.feed_size);
        let relations = agent_state.map(|st| &st.relations);
        let relations_text = agent_state
            .map(|st| st.relations.render_short())
            .unwrap_or_default();
        let beliefs_text = agent_state
            .map(|st| st.beliefs_summary())
            .unwrap_or_default();
        let fatigue = agent_state
            .map(|st| st.cognitive.fatigue)
            .unwrap_or(0.0);

        let feed = s.world.build_feed_with_relations(
            &agent_id,
            effective_feed,
            world_config.recency_weight,
            world_config.popularity_weight,
            world_config.relevance_weight,
            relations,
        );
        let feed_summaries: Vec<PostSummary> = feed
            .iter()
            .map(|p| {
                let mut summary = PostSummary::from_post(p, round, 120);
                if let Some(parent_id) = p.reply_to {
                    if let Some(parent) = s.world.posts.get(&parent_id) {
                        summary.reply_to_author = Some(parent.author_name.clone());
                    }
                }
                summary
            })
            .collect();

        let trending = s.world.trending(world_config.trending_count, 10);
        let trending_summaries: Vec<PostSummary> = trending
            .iter()
            .map(|p| PostSummary::from_post(p, round, 80))
            .collect();

        // Build reply candidates (increased to 6 to include thread candidates)
        let runtime_stance = Stance::from_sentiment(current_sentiment);
        let reply_candidates = s.world.build_reply_candidates(&agent_id, &runtime_stance, 6);

        // Collect agent's own recent posts to prevent repetition
        let own_recent_posts: Vec<String> = agent_state
            .map(|st| {
                st.post_ids.iter().rev().take(5).filter_map(|pid| {
                    s.world.posts.get(pid).map(|p| {
                        p.content.chars().take(120).collect::<String>()
                    })
                }).collect()
            })
            .unwrap_or_default();

        // Collect pending notifications
        let notifications: Vec<String> = agent_state
            .map(|st| st.pending_notifications.clone())
            .unwrap_or_default();

        let system = llm::build_single_system_prompt(&profile, current_sentiment, challenge_question.as_deref());
        let user = llm::build_single_user_prompt(
            round,
            total_rounds,
            &simulated_time,
            &memory_text,
            &feed_summaries,
            &trending_summaries,
            &reply_candidates,
            events,
            challenge_question.as_deref(),
            &own_recent_posts,
            &notifications,
            fatigue,
            &relations_text,
            &beliefs_text,
        );

        // Build ID resolution maps before releasing lock
        let post_id_map: HashMap<String, Uuid> = s.world.posts.keys()
            .map(|id| (id.to_string()[..8].to_string(), *id))
            .collect();
        let agent_id_map: HashMap<String, Uuid> = s.agents.keys()
            .map(|id| (id.to_string()[..8].to_string(), *id))
            .collect();
        let username_map: HashMap<String, Uuid> = s.agents.values()
            .map(|p| (p.username.to_lowercase(), p.id))
            .collect();

        drop(s); // Release read lock before LLM call

        match llm.call_tier(tier, &system, &user).await {
            Ok(raw) => {
                if let Some(parsed) = llm::parse_single_response(&raw) {
                    convert_single_actions(agent_id, &profile, &parsed, round, events, &post_id_map, &agent_id_map, &username_map)
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

        let agents_with_memory: Vec<(AgentProfile, String, f32, Vec<String>, Vec<String>)> = agent_ids
            .iter()
            .filter_map(|id| {
                let profile = s.agents.get(id)?.clone();
                let agent_state = s.agent_states.get(id);
                let memory = agent_state
                    .map(|st| st.memory.render_short(round, memory_recent_count))
                    .unwrap_or_default();
                let sentiment = agent_state
                    .map(|st| st.current_sentiment)
                    .unwrap_or(profile.sentiment_bias);
                // Collect agent's own recent posts to prevent repetition
                let own_posts: Vec<String> = agent_state
                    .map(|st| {
                        st.post_ids.iter().rev().take(3).filter_map(|pid| {
                            s.world.posts.get(pid).map(|p| {
                                p.content.chars().take(80).collect::<String>()
                            })
                        }).collect()
                    })
                    .unwrap_or_default();
                // Collect pending notifications
                let notifs: Vec<String> = agent_state
                    .map(|st| st.pending_notifications.clone())
                    .unwrap_or_default();
                Some((profile, memory, sentiment, own_posts, notifs))
            })
            .collect();

        let trending = s.world.trending(5, 10);
        let trending_summaries: Vec<PostSummary> = trending
            .iter()
            .map(|p| PostSummary::from_post(p, round, 60))
            .collect();

        let mut agent_contexts: Vec<llm::BatchAgentContext> = Vec::new();
        for agent_id in agent_ids {
            let Some(profile) = s.agents.get(agent_id).cloned() else {
                continue;
            };
            let agent_state = s.agent_states.get(agent_id);
            let memory = agent_state
                .map(|st| st.memory.render_short(round, memory_recent_count))
                .unwrap_or_default();
            let sentiment = agent_state
                .map(|st| st.current_sentiment)
                .unwrap_or(profile.sentiment_bias);
            let own_posts: Vec<String> = agent_state
                .map(|st| {
                    st.post_ids
                        .iter()
                        .rev()
                        .take(3)
                        .filter_map(|pid| {
                            s.world.posts.get(pid).map(|p| p.content.chars().take(80).collect::<String>())
                        })
                        .collect()
                })
                .unwrap_or_default();
            let notifications = agent_state
                .map(|st| st.pending_notifications.clone())
                .unwrap_or_default();
            let effective_feed = agent_state
                .map(|st| st.cognitive.effective_feed_size(world_config.feed_size))
                .unwrap_or(world_config.feed_size);
            let feed = s.world.build_feed_with_relations(
                agent_id,
                effective_feed,
                world_config.recency_weight,
                world_config.popularity_weight,
                world_config.relevance_weight,
                agent_state.map(|st| &st.relations),
            );
            let feed_summaries: Vec<PostSummary> = feed
                .iter()
                .take(10)
                .map(|p| {
                    let mut summary = PostSummary::from_post(p, round, 80);
                    if let Some(parent_id) = p.reply_to {
                        if let Some(parent) = s.world.posts.get(&parent_id) {
                            summary.reply_to_author = Some(parent.author_name.clone());
                        }
                    }
                    summary
                })
                .collect();
            let runtime_stance = Stance::from_sentiment(sentiment);
            let reply_candidates = s.world.build_reply_candidates(agent_id, &runtime_stance, 6);
            let fatigue = agent_state
                .map(|st| st.cognitive.fatigue)
                .unwrap_or(0.0);
            let relations_short = agent_state
                .map(|st| st.relations.render_short())
                .unwrap_or_default();
            let beliefs_short = agent_state
                .map(|st| st.beliefs_summary())
                .unwrap_or_default();
            agent_contexts.push(llm::BatchAgentContext {
                agent: profile,
                memory_short: memory,
                sentiment,
                own_posts,
                notifications,
                feed_posts: feed_summaries,
                trending_posts: trending_summaries.clone(),
                reply_candidates,
                fatigue,
                relations_short,
                beliefs_short,
            });
        }

        let system = llm::build_batch_system_prompt(&agent_contexts, persona_max, challenge_question.as_deref());
        let user = llm::build_batch_user_prompt(
            round,
            total_rounds,
            &simulated_time,
            prior_actions,
            events,
            challenge_question.as_deref(),
        );

        // Clone profiles for post-parse use
        let profiles: HashMap<String, (Uuid, AgentProfile)> = agents_with_memory
            .iter()
            .map(|(p, _, _, _, _)| (p.id.to_string()[..8].to_string(), (p.id, p.clone())))
            .collect();

        // Build ID resolution maps before releasing lock
        let post_id_map: HashMap<String, Uuid> = s.world.posts.keys()
            .map(|id| (id.to_string()[..8].to_string(), *id))
            .collect();
        let agent_id_map: HashMap<String, Uuid> = s.agents.keys()
            .map(|id| (id.to_string()[..8].to_string(), *id))
            .collect();
        let username_map: HashMap<String, Uuid> = s.agents.values()
            .map(|p| (p.username.to_lowercase(), p.id))
            .collect();

        drop(s);

        match llm.call_tier(tier, &system, &user).await {
            Ok(raw) => {
                if let Some(parsed) = llm::parse_batch_response(&raw) {
                    convert_batch_actions(&profiles, &parsed, round, &post_id_map, &agent_id_map, &username_map)
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
    post_id_map: &HashMap<String, Uuid>,
    agent_id_map: &HashMap<String, Uuid>,
    username_map: &HashMap<String, Uuid>,
) -> Vec<Action> {
    let mut actions = Vec::new();

    for pa in &parsed.actions {
        let action_type = match pa.action_type.to_lowercase().as_str() {
            "create_post" => ActionType::CreatePost,
            "reply" => ActionType::Reply,
            "like" => ActionType::Like,
            "repost" => ActionType::Repost,
            "quote_repost" => ActionType::QuoteRepost,
            "follow" => ActionType::Follow,
            "unfollow" => ActionType::Unfollow,
            "do_nothing" => ActionType::DoNothing,
            "pin_memory" => ActionType::PinMemory,
            "propose_solution" => ActionType::ProposeSolution,
            "vote_solution" => ActionType::VoteSolution,
            "refine_solution" => ActionType::RefineSolution,
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
                resolve_id(s, post_id_map).or_else(|| {
                    tracing::debug!("Unresolved target_post_id '{}' from @{}", s, profile.username);
                    None
                })
            }),
            target_agent_id: pa.target_agent_id.as_ref().and_then(|s| {
                resolve_agent_id(s, agent_id_map, username_map).or_else(|| {
                    tracing::debug!("Unresolved target_agent_id '{}' from @{}", s, profile.username);
                    None
                })
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

    // Archetype enforcement
    enforce_archetype(&profile.archetype, &mut actions);

    actions
}

fn convert_batch_actions(
    profiles: &HashMap<String, (Uuid, AgentProfile)>,
    parsed: &llm::BatchAgentResponse,
    round: u32,
    post_id_map: &HashMap<String, Uuid>,
    agent_id_map: &HashMap<String, Uuid>,
    username_map: &HashMap<String, Uuid>,
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
                "quote_repost" => ActionType::QuoteRepost,
                "follow" => ActionType::Follow,
                "unfollow" => ActionType::Unfollow,
                "do_nothing" => ActionType::DoNothing,
                "pin_memory" => ActionType::PinMemory,
                "propose_solution" => ActionType::ProposeSolution,
                "vote_solution" => ActionType::VoteSolution,
                "refine_solution" => ActionType::RefineSolution,
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
                    resolve_id(s, post_id_map).or_else(|| {
                        tracing::debug!("Unresolved target_post_id '{}' from batch @{}", s, profile.username);
                        None
                    })
                }),
                target_agent_id: pa.target_agent_id.as_ref().and_then(|s| {
                    resolve_agent_id(s, agent_id_map, username_map).or_else(|| {
                        tracing::debug!("Unresolved target_agent_id '{}' from batch @{}", s, profile.username);
                        None
                    })
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

        // Archetype enforcement for batch agents too
        let agent_actions_start = actions.len() - entry.actions.len() - if entry.pin_memory.is_some() { 1 } else { 0 };
        let mut agent_slice: Vec<Action> = actions.drain(agent_actions_start..).collect();
        enforce_archetype(&profile.archetype, &mut agent_slice);
        actions.extend(agent_slice);
    }

    actions
}

// ---------------------------------------------------------------------------
// Content deduplication
// ---------------------------------------------------------------------------

/// Remove near-duplicate posts within a tier's actions.
/// Uses Jaccard similarity on keyword sets. Duplicate posts are converted to DoNothing.
fn dedup_actions(actions: &mut Vec<Action>) {
    const STOP_WORDS: &[&str] = &[
        "the", "and", "for", "are", "but", "not", "you", "all", "can", "had",
        "her", "was", "one", "our", "out", "has", "his", "how", "its", "may",
        "new", "now", "old", "see", "way", "who", "did", "get", "got", "him",
        "let", "say", "she", "too", "use", "this", "that", "with", "have",
        "from", "they", "been", "said", "each", "will", "them", "then",
        "what", "when", "more", "some", "just", "about", "into", "over",
        "also", "than", "very", "could", "would", "should", "being", "there",
        "their", "which", "these", "those", "other", "like", "really",
        "going", "think", "make", "know", "need", "want", "take", "come",
        "people", "time", "it's", "don't", "i'm", "we're", "they're",
    ];

    fn extract_keywords(text: &str) -> HashSet<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '\'')
            .filter(|w| w.len() > 3 && !STOP_WORDS.contains(w))
            .map(|w| w.to_string())
            .collect()
    }

    // Collect indices of post/reply actions that have content
    let post_indices: Vec<usize> = actions
        .iter()
        .enumerate()
        .filter(|(_, a)| matches!(a.action_type, ActionType::CreatePost | ActionType::Reply | ActionType::ProposeSolution))
        .filter(|(_, a)| a.content.as_ref().map_or(false, |c| !c.is_empty()))
        .map(|(i, _)| i)
        .collect();

    if post_indices.len() < 2 {
        return;
    }

    let keyword_sets: Vec<(usize, HashSet<String>)> = post_indices
        .iter()
        .map(|&i| {
            let kw = extract_keywords(actions[i].content.as_deref().unwrap_or(""));
            (i, kw)
        })
        .collect();

    let mut duplicate_indices: HashSet<usize> = HashSet::new();

    // Also track word frequency across all posts to detect LLM-isms
    let mut word_freq: HashMap<String, usize> = HashMap::new();
    for (_, kws) in &keyword_sets {
        for w in kws {
            *word_freq.entry(w.clone()).or_insert(0) += 1;
        }
    }

    // Words appearing in >50% of posts are LLM-isms (e.g. "data-driven", "innovative")
    let total_posts = keyword_sets.len();
    let overused_words: HashSet<String> = word_freq
        .iter()
        .filter(|(_, &count)| count > 1 && count as f32 / total_posts as f32 > 0.5)
        .map(|(word, _)| word.clone())
        .collect();

    if !overused_words.is_empty() {
        tracing::info!(
            "Dedup: detected overused words (in >50% of posts): {:?}",
            overused_words.iter().take(10).collect::<Vec<_>>()
        );
    }

    // Jaccard similarity check (excluding overused words)
    for j in 1..keyword_sets.len() {
        let idx_j = keyword_sets[j].0;
        if duplicate_indices.contains(&idx_j) {
            continue;
        }

        let set_b: HashSet<&String> = keyword_sets[j]
            .1
            .iter()
            .filter(|w| !overused_words.contains(*w))
            .collect();

        for i in 0..j {
            let idx_i = keyword_sets[i].0;
            if duplicate_indices.contains(&idx_i) {
                continue;
            }

            let set_a: HashSet<&String> = keyword_sets[i]
                .1
                .iter()
                .filter(|w| !overused_words.contains(*w))
                .collect();

            if set_a.is_empty() || set_b.is_empty() {
                continue;
            }

            let intersection = set_a.intersection(&set_b).count();
            let union = set_a.union(&set_b).count();
            let similarity = intersection as f32 / union as f32;

            if similarity > 0.35 {
                duplicate_indices.insert(idx_j);
                break;
            }
        }
    }

    // Also flag posts that are composed mostly of overused words (LLM slop)
    if !overused_words.is_empty() {
        for (idx, kws) in &keyword_sets {
            if duplicate_indices.contains(idx) {
                continue;
            }
            if kws.is_empty() {
                continue;
            }
            let overused_count = kws.iter().filter(|w| overused_words.contains(*w)).count();
            // If >60% of this post's keywords are overused, it's generic slop
            if overused_count as f32 / kws.len() as f32 > 0.6 && kws.len() > 3 {
                duplicate_indices.insert(*idx);
            }
        }
    }

    let deduped_count = duplicate_indices.len();
    if deduped_count > 0 {
        for &i in &duplicate_indices {
            tracing::debug!(
                "Dedup: converting duplicate from @{} to do_nothing: {:?}",
                actions[i].agent_name,
                actions[i].content.as_deref().unwrap_or("").chars().take(60).collect::<String>()
            );
            actions[i].action_type = ActionType::DoNothing;
            actions[i].content = None;
        }
        tracing::info!(
            "Dedup: removed {deduped_count}/{} near-duplicate posts in tier",
            post_indices.len()
        );
    }
}

// ---------------------------------------------------------------------------
// Apply action to world state
// ---------------------------------------------------------------------------

fn apply_action(state: &mut SimulationState, action: &Action) {
    // Sanitize content before storing
    let clean_content = action.content.as_ref().map(|c| sanitize_content(c, state));

    match &action.action_type {
        ActionType::CreatePost => {
            if let Some(content) = &clean_content {
                let post = Post {
                    id: action.id,
                    author_id: action.agent_id,
                    author_name: action.agent_name.clone(),
                    content: content.clone(),
                    created_at_round: action.round,
                    simulated_time: action.simulated_time,
                    reply_to: None,
                    repost_of: None,
                    quote_of: None,
                    refines: None,
                    likes: Vec::new(),
                    replies: Vec::new(),
                    reposts: Vec::new(),
                    hashtags: extract_hashtags(content),
                    cascade_depth: 0,
                    cascade_root: None,
                    contested: false,
                    opposing_reply_count: 0,
                };
                state.world.add_post(post);
                if let Some(agent_state) = state.agent_states.get_mut(&action.agent_id) {
                    agent_state.post_ids.push(action.id);
                }
            }
        }
        ActionType::Reply => {
            if let (Some(content), Some(target)) = (&clean_content, &action.target_post_id) {
                let post = Post {
                    id: action.id,
                    author_id: action.agent_id,
                    author_name: action.agent_name.clone(),
                    content: content.clone(),
                    created_at_round: action.round,
                    simulated_time: action.simulated_time,
                    reply_to: Some(*target),
                    repost_of: None,
                    quote_of: None,
                    refines: None,
                    likes: Vec::new(),
                    replies: Vec::new(),
                    reposts: Vec::new(),
                    hashtags: Vec::new(),
                    cascade_depth: 0,
                    cascade_root: None,
                    contested: false,
                    opposing_reply_count: 0,
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
                    content: clean_content.clone().unwrap_or_default(),
                    created_at_round: action.round,
                    simulated_time: action.simulated_time,
                    reply_to: None,
                    repost_of: Some(target),
                    quote_of: None,
                    refines: None,
                    likes: Vec::new(),
                    replies: Vec::new(),
                    reposts: Vec::new(),
                    hashtags: Vec::new(),
                    cascade_depth: 0,
                    cascade_root: None,
                    contested: false,
                    opposing_reply_count: 0,
                };
                state.world.add_repost(target, repost);
            }
        }
        ActionType::QuoteRepost => {
            if let Some(target) = action.target_post_id {
                let post = Post {
                    id: action.id,
                    author_id: action.agent_id,
                    author_name: action.agent_name.clone(),
                    content: clean_content.clone().unwrap_or_default(),
                    created_at_round: action.round,
                    simulated_time: action.simulated_time,
                    reply_to: None,
                    repost_of: None,
                    quote_of: Some(target),
                    refines: None,
                    likes: Vec::new(),
                    replies: Vec::new(),
                    reposts: Vec::new(),
                    hashtags: Vec::new(),
                    cascade_depth: 0,
                    cascade_root: None,
                    contested: false,
                    opposing_reply_count: 0,
                };
                state.world.add_post(post);
                // Also count as repost on the original
                if let Some(original) = state.world.posts.get_mut(&target) {
                    original.reposts.push(action.id);
                }
                if let Some(agent_state) = state.agent_states.get_mut(&action.agent_id) {
                    agent_state.post_ids.push(action.id);
                }
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
        ActionType::ProposeSolution => {
            if let Some(content) = &clean_content {
                let post = Post {
                    id: action.id,
                    author_id: action.agent_id,
                    author_name: action.agent_name.clone(),
                    content: content.clone(),
                    created_at_round: action.round,
                    simulated_time: action.simulated_time,
                    reply_to: None,
                    repost_of: None,
                    quote_of: None,
                    refines: None,
                    likes: Vec::new(),
                    replies: Vec::new(),
                    reposts: Vec::new(),
                    hashtags: extract_hashtags(content),
                    cascade_depth: 0,
                    cascade_root: None,
                    contested: false,
                    opposing_reply_count: 0,
                };
                state.world.add_post(post);
                state.world.solution_ids.push(action.id);
                if let Some(agent_state) = state.agent_states.get_mut(&action.agent_id) {
                    agent_state.post_ids.push(action.id);
                }
            }
        }
        ActionType::VoteSolution => {
            if let Some(target) = action.target_post_id {
                // Only vote if it's a known solution and agent hasn't voted yet
                if state.world.solution_ids.contains(&target) {
                    let voters = state.world.solution_votes.entry(target).or_default();
                    if !voters.contains(&action.agent_id) {
                        voters.push(action.agent_id);
                    }
                    // Also count as a like on the solution post
                    state.world.add_like(target, action.agent_id);
                }
            }
        }
        ActionType::RefineSolution => {
            if let Some(content) = &clean_content {
                let target = action.target_post_id;
                let post = Post {
                    id: action.id,
                    author_id: action.agent_id,
                    author_name: action.agent_name.clone(),
                    content: content.clone(),
                    created_at_round: action.round,
                    simulated_time: action.simulated_time,
                    reply_to: None,
                    repost_of: None,
                    quote_of: None,
                    refines: target,
                    likes: Vec::new(),
                    replies: Vec::new(),
                    reposts: Vec::new(),
                    hashtags: extract_hashtags(content),
                    cascade_depth: 0,
                    cascade_root: None,
                    contested: false,
                    opposing_reply_count: 0,
                };
                state.world.add_post(post);
                state.world.solution_ids.push(action.id);
                if let Some(agent_state) = state.agent_states.get_mut(&action.agent_id) {
                    agent_state.post_ids.push(action.id);
                }
            }
        }
        ActionType::DoNothing => {}
    }

    // Update agent memory with rich observation (includes context)
    if !matches!(action.action_type, ActionType::DoNothing | ActionType::PinMemory) {
        let rich_desc = describe_action_rich(action, state);
        if let Some(agent_state) = state.agent_states.get_mut(&action.agent_id) {
            agent_state.memory.observe(action.round, rich_desc);
        }
    }

    // Log to agent's action history
    if !matches!(action.action_type, ActionType::DoNothing) {
        if let Some(agent_state) = state.agent_states.get_mut(&action.agent_id) {
            agent_state.log_action(ActionLogEntry {
                round: action.round,
                action_type: action.action_type.to_string(),
                content: action.content.as_ref().map(|c| {
                    if c.len() > 200 {
                        format!("{}...", &c[..200])
                    } else {
                        c.clone()
                    }
                }),
            });
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
        ActionType::QuoteRepost => {
            let preview = action
                .content
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(60)
                .collect::<String>();
            format!("quote-reposted: \"{preview}\"")
        }
        ActionType::Follow => format!("followed someone"),
        ActionType::Unfollow => format!("unfollowed someone"),
        ActionType::DoNothing => format!("idle"),
        ActionType::PinMemory => format!("pinned a memory"),
        ActionType::ProposeSolution => {
            let preview = action
                .content
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(60)
                .collect::<String>();
            format!("proposed solution: \"{preview}\"")
        }
        ActionType::VoteSolution => format!("voted on a solution"),
        ActionType::RefineSolution => {
            let preview = action
                .content
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(60)
                .collect::<String>();
            format!("refined solution: \"{preview}\"")
        }
    }
}

fn describe_action_for_context(action: &Action) -> String {
    format!("@{} {}", action.agent_name, describe_action(action))
}

/// Resolve a post ID from LLM response — tries full UUID, then short ID (first 8 chars).
fn resolve_id(s: &str, id_map: &HashMap<String, Uuid>) -> Option<Uuid> {
    let s = s.trim();
    if let Ok(uuid) = Uuid::parse_str(s) {
        return Some(uuid);
    }
    let short = if s.len() >= 8 { &s[..8] } else { s };
    id_map.get(short).copied()
}

/// Resolve an agent ID — tries UUID, short ID, then @username.
fn resolve_agent_id(s: &str, agent_id_map: &HashMap<String, Uuid>, username_map: &HashMap<String, Uuid>) -> Option<Uuid> {
    let s = s.trim();
    if let Ok(uuid) = Uuid::parse_str(s) {
        return Some(uuid);
    }
    let short = if s.len() >= 8 { &s[..8] } else { s };
    if let Some(id) = agent_id_map.get(short) {
        return Some(*id);
    }
    let username = s.trim_start_matches('@').to_lowercase();
    username_map.get(&username).copied()
}

/// Rich action description with context for agent memory.
fn describe_action_rich(action: &Action, state: &SimulationState) -> String {
    match &action.action_type {
        ActionType::CreatePost => {
            let preview = action
                .content
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(80)
                .collect::<String>();
            format!("I posted: \"{preview}\"")
        }
        ActionType::Reply => {
            let my_reply = action
                .content
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(60)
                .collect::<String>();
            if let Some(target_id) = action.target_post_id {
                if let Some(target_post) = state.world.posts.get(&target_id) {
                    let target_preview: String = target_post.content.chars().take(60).collect();
                    format!(
                        "I replied to @{} who said \"{}...\" with \"{}\"",
                        target_post.author_name, target_preview, my_reply
                    )
                } else {
                    format!("I replied: \"{my_reply}\"")
                }
            } else {
                format!("I replied: \"{my_reply}\"")
            }
        }
        ActionType::Like => {
            if let Some(target_id) = action.target_post_id {
                if let Some(target_post) = state.world.posts.get(&target_id) {
                    let preview: String = target_post.content.chars().take(50).collect();
                    format!("I liked @{}'s post: \"{}...\"", target_post.author_name, preview)
                } else {
                    "I liked a post".into()
                }
            } else {
                "I liked a post".into()
            }
        }
        ActionType::Repost => {
            if let Some(target_id) = action.target_post_id {
                if let Some(target_post) = state.world.posts.get(&target_id) {
                    let preview: String = target_post.content.chars().take(50).collect();
                    format!("I reposted @{}: \"{}...\"", target_post.author_name, preview)
                } else {
                    "I reposted".into()
                }
            } else {
                "I reposted".into()
            }
        }
        ActionType::Follow => {
            if let Some(target_id) = action.target_agent_id {
                if let Some(target_profile) = state.agents.get(&target_id) {
                    format!("I followed @{} ({})", target_profile.username, Stance::from_sentiment(
                        state.agent_states.get(&target_id).map(|s| s.current_sentiment).unwrap_or(0.0)
                    ))
                } else {
                    "I followed someone".into()
                }
            } else {
                "I followed someone".into()
            }
        }
        ActionType::QuoteRepost => {
            let my_comment = action
                .content
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(60)
                .collect::<String>();
            if let Some(target_id) = action.target_post_id {
                if let Some(target_post) = state.world.posts.get(&target_id) {
                    let preview: String = target_post.content.chars().take(50).collect();
                    format!("I quote-reposted @{} (\"{}...\") saying \"{}\"", target_post.author_name, preview, my_comment)
                } else {
                    format!("I quote-reposted: \"{my_comment}\"")
                }
            } else {
                format!("I quote-reposted: \"{my_comment}\"")
            }
        }
        ActionType::Unfollow => "I unfollowed someone".into(),
        ActionType::DoNothing => "idle".into(),
        ActionType::PinMemory => "pinned a memory".into(),
        ActionType::ProposeSolution => {
            let preview = action
                .content
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(80)
                .collect::<String>();
            format!("I proposed a solution: \"{preview}\"")
        }
        ActionType::VoteSolution => {
            if let Some(target_id) = action.target_post_id {
                if let Some(target_post) = state.world.posts.get(&target_id) {
                    let preview: String = target_post.content.chars().take(50).collect();
                    format!("I voted for @{}'s solution: \"{}...\"", target_post.author_name, preview)
                } else {
                    "I voted on a solution".into()
                }
            } else {
                "I voted on a solution".into()
            }
        }
        ActionType::RefineSolution => {
            let preview = action
                .content
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(80)
                .collect::<String>();
            format!("I refined a solution: \"{preview}\"")
        }
    }
}

/// Update agent sentiments based on interactions this round.
async fn update_sentiments(state: &SharedState, round: u32) {
    let mut s = state.write().await;

    // Collect interaction data for each agent this round
    let agent_ids: Vec<Uuid> = s.agent_states.keys().cloned().collect();

    for agent_id in &agent_ids {
        // Find posts this agent interacted with this round (liked or replied to)
        let interactions: Vec<f32> = {
            let agent_state = match s.agent_states.get(agent_id) {
                Some(st) => st,
                None => continue,
            };

            // Look at recent action log entries from this round
            agent_state.action_log.iter()
                .filter(|entry| entry.round == round)
                .filter_map(|entry| {
                    // Get the sentiment of the content the agent interacted with
                    match entry.action_type.as_str() {
                        "like" | "reply" | "repost" => {
                            // Simple sentiment heuristic from content
                            if let Some(content) = &entry.content {
                                Some(estimate_content_sentiment(content))
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                })
                .collect()
        };

        if interactions.is_empty() {
            continue;
        }

        let exposure_avg = interactions.iter().sum::<f32>() / interactions.len() as f32;

        let profile = match s.agents.get(agent_id) {
            Some(p) => p,
            None => continue,
        };
        let conviction = profile.sentiment_bias.abs();
        let drift_rate = 0.05 * (1.0 - conviction * 0.5);

        let agent_state = match s.agent_states.get_mut(agent_id) {
            Some(st) => st,
            None => continue,
        };

        let pull = exposure_avg - agent_state.current_sentiment;
        agent_state.current_sentiment =
            (agent_state.current_sentiment + pull * drift_rate).clamp(-1.0, 1.0);
        agent_state.sentiment_history.push((round, agent_state.current_sentiment));
    }
}

/// Simple heuristic to estimate sentiment of content (-1.0 to 1.0).
/// Estimate sentiment of content (-1.0 to 1.0) using weighted word lists and structural cues.
/// Goes beyond simple keyword matching with intensity weights and negation detection.
fn estimate_content_sentiment(content: &str) -> f32 {
    let lower = content.to_lowercase();

    // Weighted positive words: (word, weight)
    let positive: &[(&str, f32)] = &[
        ("great", 0.6), ("good", 0.4), ("amazing", 0.8), ("love", 0.7),
        ("excellent", 0.8), ("progress", 0.5), ("exciting", 0.6),
        ("brilliant", 0.7), ("impressive", 0.6), ("opportunity", 0.5),
        ("support", 0.4), ("fantastic", 0.7), ("wonderful", 0.7),
        ("helpful", 0.5), ("hope", 0.4), ("happy", 0.5), ("agree", 0.4),
        ("exactly", 0.3), ("right", 0.3), ("nice", 0.3), ("glad", 0.4),
        ("fair", 0.3), ("trust", 0.4), ("proud", 0.5), ("win", 0.5),
        ("success", 0.5), ("improve", 0.4), ("benefit", 0.4),
    ];
    let negative: &[(&str, f32)] = &[
        ("terrible", 0.8), ("awful", 0.7), ("hate", 0.7), ("worst", 0.8),
        ("greed", 0.6), ("scam", 0.7), ("disaster", 0.8), ("outrage", 0.7),
        ("betrayal", 0.7), ("unacceptable", 0.7), ("horrible", 0.8),
        ("disgusting", 0.7), ("shame", 0.5), ("pathetic", 0.6),
        ("ridiculous", 0.5), ("angry", 0.5), ("fear", 0.4), ("worried", 0.4),
        ("wrong", 0.4), ("fail", 0.5), ("broken", 0.5), ("stupid", 0.5),
        ("toxic", 0.6), ("corrupt", 0.6), ("lie", 0.5), ("liar", 0.6),
        ("damage", 0.5), ("destroy", 0.6), ("threat", 0.5), ("danger", 0.5),
        ("mess", 0.4), ("trash", 0.5), ("garbage", 0.5), ("suck", 0.5),
    ];

    let mut pos_score: f32 = 0.0;
    let mut neg_score: f32 = 0.0;

    for &(word, weight) in positive {
        if lower.contains(word) {
            pos_score += weight;
        }
    }
    for &(word, weight) in negative {
        if lower.contains(word) {
            neg_score += weight;
        }
    }

    // Structural cues
    let exclamation_count = content.chars().filter(|&c| c == '!').count();
    let question_mark = content.contains('?');
    let all_caps_words = content.split_whitespace().filter(|w| w.len() > 2 && *w == w.to_uppercase()).count();

    // Exclamation marks amplify existing sentiment
    let intensity_mult = 1.0 + (exclamation_count as f32 * 0.1).min(0.3);
    pos_score *= intensity_mult;
    neg_score *= intensity_mult;

    // ALL CAPS words suggest strong emotion (amplify whichever is dominant)
    if all_caps_words > 0 {
        let caps_boost = (all_caps_words as f32 * 0.1).min(0.3);
        if pos_score > neg_score { pos_score += caps_boost; }
        else { neg_score += caps_boost; }
    }

    // Simple negation: "not good" flips sentiment
    let negation_words = ["not ", "don't ", "doesn't ", "isn't ", "aren't ", "wasn't ",
        "won't ", "can't ", "never ", "no "];
    let has_negation = negation_words.iter().any(|n| lower.contains(n));
    if has_negation && (pos_score > 0.0 || neg_score > 0.0) {
        std::mem::swap(&mut pos_score, &mut neg_score);
        // Dampen the flip (negation is weaker than direct statement)
        pos_score *= 0.7;
        neg_score *= 0.7;
    }

    // Questions with no clear sentiment lean neutral
    if question_mark && pos_score + neg_score < 0.5 {
        return 0.0;
    }

    let total = pos_score + neg_score;
    if total == 0.0 {
        return 0.0;
    }

    ((pos_score - neg_score) / total).clamp(-1.0, 1.0)
}

/// Update cognitive state (fatigue/attention) and relational memory after a round.
async fn update_cognitive_and_relations(state: &SharedState, actions: &[Action], round: u32) {
    let mut s = state.write().await;

    // Track which agents were active this round and how many actions they took
    let mut action_counts: HashMap<Uuid, u32> = HashMap::new();
    for action in actions {
        if !matches!(action.action_type, ActionType::DoNothing | ActionType::PinMemory) {
            *action_counts.entry(action.agent_id).or_insert(0) += 1;
        }
    }

    // Update cognitive state for all agents
    let all_agents: Vec<Uuid> = s.agent_states.keys().cloned().collect();
    for agent_id in &all_agents {
        if let Some(agent_state) = s.agent_states.get_mut(agent_id) {
            if let Some(&count) = action_counts.get(agent_id) {
                agent_state.cognitive.on_active_round(count);
            } else {
                agent_state.cognitive.on_idle_round();
            }
            // Decay relational memory
            agent_state.relations.decay(round);
        }
    }

    // Update relational memory based on interactions
    for action in actions {
        match &action.action_type {
            ActionType::Like | ActionType::Repost => {
                if let Some(target_id) = action.target_post_id {
                    // Extract needed values before mutable borrow
                    let post_info = s.world.posts.get(&target_id)
                        .map(|p| (p.author_id, p.engagement_score()));
                    if let Some((author_id, engagement)) = post_info {
                        if author_id != action.agent_id {
                            if let Some(agent_state) = s.agent_states.get_mut(&action.agent_id) {
                                agent_state.relations.record_positive(author_id, round);
                                agent_state.relations.update_influence(author_id, engagement);
                            }
                        }
                    }
                }
            }
            ActionType::Reply => {
                if let Some(target_id) = action.target_post_id {
                    // Extract needed values before mutable borrow
                    let post_info = s.world.posts.get(&target_id)
                        .map(|p| (p.author_id, estimate_content_sentiment(&p.content)));
                    if let Some((author_id, post_sentiment)) = post_info {
                        if author_id != action.agent_id {
                            let reply_sentiment = action.content.as_ref()
                                .map(|c| estimate_content_sentiment(c))
                                .unwrap_or(0.0);
                            let aligned = (reply_sentiment * post_sentiment) > 0.0
                                || reply_sentiment.abs() < 0.1;

                            if let Some(agent_state) = s.agent_states.get_mut(&action.agent_id) {
                                if aligned {
                                    agent_state.relations.record_positive(author_id, round);
                                } else {
                                    agent_state.relations.record_negative(author_id, round);
                                }
                            }
                        }
                    }
                }
            }
            ActionType::Follow => {
                if let Some(target_id) = action.target_agent_id {
                    if let Some(agent_state) = s.agent_states.get_mut(&action.agent_id) {
                        agent_state.relations.record_positive(target_id, round);
                    }
                }
            }
            ActionType::Unfollow => {
                if let Some(target_id) = action.target_agent_id {
                    if let Some(agent_state) = s.agent_states.get_mut(&action.agent_id) {
                        agent_state.relations.record_negative(target_id, round);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Update agent beliefs based on content exposure, and mark contested posts.
async fn update_beliefs_and_contested(state: &SharedState, actions: &[Action], round: u32) {
    let mut s = state.write().await;

    // Extract keywords from content for belief tracking
    fn extract_belief_topics(content: &str) -> Vec<String> {
        let lower = content.to_lowercase();
        // Key topic words that represent debatable positions
        let topic_markers = [
            "ai", "automation", "layoffs", "jobs", "workers",
            "technology", "regulation", "safety", "privacy", "economy",
            "climate", "equality", "freedom", "innovation", "corporate",
            "government", "education", "healthcare", "immigration", "rights",
        ];
        topic_markers.iter()
            .filter(|&&t| lower.contains(t))
            .map(|&s| s.to_string())
            .collect()
    }

    // For each reply, check if it opposes the parent post -> mark contested
    for action in actions {
        if matches!(action.action_type, ActionType::Reply) {
            if let (Some(target_id), Some(reply_content)) = (action.target_post_id, &action.content) {
                // Get parent sentiment before mutable borrow
                let parent_sentiment = s.world.posts.get(&target_id)
                    .map(|p| estimate_content_sentiment(&p.content));

                if let Some(parent_sent) = parent_sentiment {
                    let reply_sent = estimate_content_sentiment(reply_content);
                    // Opposing reply: sentiments have different signs and both are significant
                    if parent_sent * reply_sent < -0.05 && parent_sent.abs() > 0.1 && reply_sent.abs() > 0.1 {
                        s.world.mark_contested(target_id);
                    }
                }
            }
        }
    }

    // Update beliefs for agents who saw content this round (via feed or interactions)
    for action in actions {
        let content = match &action.content {
            Some(c) if !c.is_empty() => c.clone(),
            _ => continue,
        };

        let topics = extract_belief_topics(&content);
        if topics.is_empty() {
            continue;
        }

        let content_sentiment = estimate_content_sentiment(&content);
        let author_id = action.agent_id;

        // Update the author's own beliefs (writing reinforces belief)
        if let Some(agent_state) = s.agent_states.get_mut(&author_id) {
            for topic in &topics {
                agent_state.update_belief(topic, content_sentiment, 0.5); // self-reinforcement
            }
        }

        // For likes/reposts, update the liker's beliefs (exposure)
        if matches!(action.action_type, ActionType::Like | ActionType::Repost) {
            if let Some(target_id) = action.target_post_id {
                let target_info = s.world.posts.get(&target_id)
                    .map(|p| (estimate_content_sentiment(&p.content), p.author_id));

                if let Some((target_sent, target_author)) = target_info {
                    let target_topics: Vec<String> = s.world.posts.get(&target_id)
                        .map(|p| extract_belief_topics(&p.content))
                        .unwrap_or_default();

                    // Trust in the target author
                    let trust = s.agent_states.get(&action.agent_id)
                        .map(|st| st.relations.trust_for(&target_author))
                        .unwrap_or(0.0);

                    if let Some(agent_state) = s.agent_states.get_mut(&action.agent_id) {
                        for topic in &target_topics {
                            agent_state.update_belief(topic, target_sent, trust);
                        }
                    }
                }
            }
        }
    }
}

fn extract_hashtags(content: &str) -> Vec<String> {
    content
        .split_whitespace()
        .filter(|w| w.starts_with('#') && w.len() > 1)
        .map(|w| w.to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Archetype enforcement — trim actions to archetype limits
// ---------------------------------------------------------------------------

fn enforce_archetype(archetype: &BehaviorArchetype, actions: &mut Vec<Action>) {
    // Without cognitive state, use base max
    enforce_archetype_with_fatigue(archetype, actions, 0.0);
}

fn enforce_archetype_with_fatigue(archetype: &BehaviorArchetype, actions: &mut Vec<Action>, _fatigue: f32) {
    let max = archetype.max_actions();

    // For engagement-only archetypes (Lurker, Cheerleader), convert create_post to do_nothing
    if archetype.prefers_engagement_only() {
        for action in actions.iter_mut() {
            if matches!(action.action_type, ActionType::CreatePost) {
                // Allow very short replies/posts from Cheerleader (they cheer)
                if matches!(archetype, BehaviorArchetype::Lurker) {
                    action.action_type = ActionType::DoNothing;
                    action.content = None;
                }
            }
        }
    }

    // Enforce max post length per archetype
    let max_len = archetype.max_post_length() as usize;
    for action in actions.iter_mut() {
        if let Some(content) = &action.content {
            if content.len() > max_len && matches!(
                action.action_type,
                ActionType::CreatePost | ActionType::Reply | ActionType::QuoteRepost
            ) {
                let truncated: String = content.chars().take(max_len).collect();
                action.content = Some(truncated);
            }
        }
    }

    // Count real actions (excluding DoNothing and PinMemory)
    let real_actions: Vec<usize> = actions
        .iter()
        .enumerate()
        .filter(|(_, a)| !matches!(a.action_type, ActionType::DoNothing | ActionType::PinMemory))
        .map(|(i, _)| i)
        .collect();

    if real_actions.len() > max {
        // Keep first `max` real actions, convert rest to DoNothing
        for &idx in &real_actions[max..] {
            actions[idx].action_type = ActionType::DoNothing;
            actions[idx].content = None;
        }
    }
}

// ---------------------------------------------------------------------------
// Content sanitization — strip leaked 8-char hex IDs from post content
// ---------------------------------------------------------------------------

fn sanitize_content(content: &str, state: &SimulationState) -> String {
    let re = Regex::new(r"\b[0-9a-f]{8}\b").unwrap();
    let result = re.replace_all(content, |caps: &regex_lite::Captures| {
        let matched = caps.get(0).unwrap().as_str();
        // Check if this matches a known post or agent ID prefix
        let is_post_id = state.world.posts.keys().any(|id| id.to_string().starts_with(matched));
        let is_agent_id = state.agents.keys().any(|id| id.to_string().starts_with(matched));
        if is_post_id || is_agent_id {
            String::new() // Strip it
        } else {
            matched.to_string() // Keep it (could be legit hex like a color code)
        }
    });
    // Clean up double spaces left by removal
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// Engagement notifications — build notifications for next round
// ---------------------------------------------------------------------------

async fn build_notifications(state: &SharedState, actions: &[Action], _round: u32) {
    let mut notifications: HashMap<Uuid, Vec<String>> = HashMap::new();

    // Gather who got likes, replies, reposts this round
    let s = state.read().await;
    for action in actions {
        match &action.action_type {
            ActionType::Like => {
                if let Some(target_id) = action.target_post_id {
                    if let Some(post) = s.world.posts.get(&target_id) {
                        if post.author_id != action.agent_id {
                            notifications.entry(post.author_id).or_default()
                                .push(format!("@{} liked your post", action.agent_name));
                        }
                    }
                }
            }
            ActionType::Reply => {
                if let Some(target_id) = action.target_post_id {
                    if let Some(post) = s.world.posts.get(&target_id) {
                        if post.author_id != action.agent_id {
                            let preview = action.content.as_deref().unwrap_or("").chars().take(40).collect::<String>();
                            notifications.entry(post.author_id).or_default()
                                .push(format!("@{} replied: \"{}\"", action.agent_name, preview));
                        }
                    }
                }
            }
            ActionType::Repost | ActionType::QuoteRepost => {
                if let Some(target_id) = action.target_post_id {
                    if let Some(post) = s.world.posts.get(&target_id) {
                        if post.author_id != action.agent_id {
                            notifications.entry(post.author_id).or_default()
                                .push(format!("@{} reposted your post", action.agent_name));
                        }
                    }
                }
            }
            ActionType::Follow => {
                if let Some(target_id) = action.target_agent_id {
                    notifications.entry(target_id).or_default()
                        .push(format!("@{} followed you", action.agent_name));
                }
            }
            ActionType::VoteSolution => {
                if let Some(target_id) = action.target_post_id {
                    if let Some(post) = s.world.posts.get(&target_id) {
                        if post.author_id != action.agent_id {
                            notifications.entry(post.author_id).or_default()
                                .push(format!("@{} voted for your solution", action.agent_name));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    drop(s);

    // Consolidate and store notifications — max 5 per agent
    let mut s = state.write().await;
    for (agent_id, notifs) in notifications {
        if let Some(agent_state) = s.agent_states.get_mut(&agent_id) {
            let consolidated = if notifs.len() > 5 {
                let mut summary = notifs[..3].to_vec();
                summary.push(format!("...and {} more notifications", notifs.len() - 3));
                summary
            } else {
                notifs
            };
            agent_state.pending_notifications = consolidated;
        }
    }
}

// ---------------------------------------------------------------------------
// Webhook fire-and-forget
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// State validation — check consistency after each round
// ---------------------------------------------------------------------------

/// Validate world state consistency. Returns list of issues found (empty = OK).
pub fn validate_state(state: &SimulationState) -> Vec<String> {
    let mut issues = Vec::new();

    // Check: all agent_states have corresponding agents
    for id in state.agent_states.keys() {
        if !state.agents.contains_key(id) {
            issues.push(format!("Orphan agent_state: {}", &id.to_string()[..8]));
        }
    }

    // Check: all agents have agent_states
    for id in state.agents.keys() {
        if !state.agent_states.contains_key(id) {
            issues.push(format!("Missing agent_state for: {}", &id.to_string()[..8]));
        }
    }

    // Check: post authors exist in agents (except system posts)
    for post in state.world.posts.values() {
        if !post.author_id.is_nil() && !state.agents.contains_key(&post.author_id) {
            issues.push(format!("Post {} has unknown author {}", post.short_id(), &post.author_id.to_string()[..8]));
        }
    }

    // Check: reply_to references exist
    for post in state.world.posts.values() {
        if let Some(parent_id) = post.reply_to {
            if !state.world.posts.contains_key(&parent_id) {
                issues.push(format!("Post {} replies to missing post {}", post.short_id(), &parent_id.to_string()[..8]));
            }
        }
    }

    // Check: sentiment within valid range
    for (id, agent_state) in &state.agent_states {
        if agent_state.current_sentiment < -1.0 || agent_state.current_sentiment > 1.0 {
            issues.push(format!("Agent {} sentiment out of range: {:.2}", &id.to_string()[..8], agent_state.current_sentiment));
        }
        if agent_state.cognitive.fatigue < 0.0 || agent_state.cognitive.fatigue > 1.0 {
            issues.push(format!("Agent {} fatigue out of range: {:.2}", &id.to_string()[..8], agent_state.cognitive.fatigue));
        }
    }

    // Check: social graph consistency (following/followers mirror)
    let total_following: usize = state.world.social_graph.following.values().map(|v| v.len()).sum();
    let total_followers: usize = state.world.social_graph.followers.values().map(|v| v.len()).sum();
    if total_following != total_followers {
        issues.push(format!("Social graph inconsistency: {} following vs {} followers", total_following, total_followers));
    }

    // Check: no self-follows
    for (follower, targets) in &state.world.social_graph.following {
        if targets.contains(follower) {
            issues.push(format!("Self-follow detected: {}", &follower.to_string()[..8]));
        }
    }

    if !issues.is_empty() {
        tracing::warn!("State validation found {} issues", issues.len());
        for issue in &issues {
            tracing::warn!("  - {issue}");
        }
    }

    issues
}

fn fire_webhook_if_needed(config: &SimConfig, event_type: &str, payload: &serde_json::Value) {
    if !config.webhooks.enabled {
        return;
    }
    let Some(url) = &config.webhooks.url else { return };
    if !config.webhooks.events.is_empty() && !config.webhooks.events.iter().any(|e| e == event_type) {
        return;
    }
    let url = url.clone();
    let body = serde_json::json!({
        "event": event_type,
        "data": payload,
    });
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        if let Err(e) = client.post(&url).json(&body).send().await {
            tracing::warn!("Webhook POST to {url} failed: {e}");
        }
    });
}
