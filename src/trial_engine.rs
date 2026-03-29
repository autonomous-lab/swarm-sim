//! Trial-specific engine logic — executes courtroom rounds with proper
//! procedural structure, witness Q&A, judge rulings, and differentiated
//! jury impact.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::broadcast;
use uuid::Uuid;

use crate::agent::Tier;
use crate::engine::{SharedState, WsEvent, TrialJurorSnapshot};
use crate::llm::LlmClient;
use crate::trial::*;
use crate::world::{Action, ActionType, RoundSummary};

/// Execute a single trial round.
pub async fn execute_trial_round(
    state: SharedState,
    llm: Arc<LlmClient>,
    ws_tx: &broadcast::Sender<WsEvent>,
    round: u32,
) -> anyhow::Result<RoundSummary> {
    // 1. Determine phase and update
    let (phase, case_summary, schedule) = {
        let s = state.read().await;
        let trial = s.trial.as_ref().expect("trial mode without trial state");
        (
            trial.schedule.phase_for_round(round),
            s.config.simulation.scenario_prompt.clone(),
            trial.schedule.clone(),
        )
    };

    // Phase change broadcast
    {
        let mut s = state.write().await;
        if let Some(ref mut trial) = s.trial {
            let prev = trial.current_phase;
            trial.current_phase = phase;
            if prev != phase {
                let _ = ws_tx.send(WsEvent::TrialPhaseChange {
                    phase: format!("{}", phase),
                    round,
                });
            }
        }
    }

    let mut total_actions = 0;

    // 2. Get court participants
    let participants = get_participants(&state).await;

    // 3. Build transcript context (last 8 entries)
    let recent_transcript = get_recent_transcript(&state, 8).await;

    // 4. Get arguments already made by each attorney (for anti-repetition)
    let (pros_prior_args, def_prior_args) = get_prior_arguments(&state).await;

    // 5. Get jury status string
    let jury_status = get_jury_status_string(&state).await;

    // 6. Execute phase-specific flow
    match phase {
        TrialPhase::Opening => {
            // Judge opens → Prosecution opening → Defense opening
            total_actions += exec_judge(&state, &llm, ws_tx, round, phase, &case_summary,
                &participants, &recent_transcript, &[]).await;

            let transcript_after_judge = get_recent_transcript(&state, 3).await;
            total_actions += exec_attorney(&state, &llm, ws_tx, round, phase, &case_summary,
                &participants, Party::Prosecution, &transcript_after_judge,
                &pros_prior_args, &jury_status).await;

            let transcript_after_pros = get_recent_transcript(&state, 5).await;
            total_actions += exec_attorney(&state, &llm, ws_tx, round, phase, &case_summary,
                &participants, Party::Defense, &transcript_after_pros,
                &def_prior_args, &jury_status).await;
        }

        TrialPhase::ProsecutionCase => {
            // Judge directs → Prosecution presents/examines → Witness answers → Defense cross-examines
            total_actions += exec_judge(&state, &llm, ws_tx, round, phase, &case_summary,
                &participants, &recent_transcript, &[]).await;

            let t = get_recent_transcript(&state, 5).await;
            total_actions += exec_attorney(&state, &llm, ws_tx, round, phase, &case_summary,
                &participants, Party::Prosecution, &t,
                &pros_prior_args, &jury_status).await;

            // Witness responds to what prosecution just said
            let t = get_recent_transcript(&state, 4).await;
            total_actions += exec_witness(&state, &llm, ws_tx, round, &case_summary,
                &participants, Party::Prosecution, &t).await;

            // Defense cross-examines
            let t = get_recent_transcript(&state, 6).await;
            total_actions += exec_attorney(&state, &llm, ws_tx, round, phase, &case_summary,
                &participants, Party::Defense, &t,
                &def_prior_args, &jury_status).await;
        }

        TrialPhase::DefenseCase => {
            // Judge directs → Defense presents/examines → Witness answers → Prosecution cross-examines
            total_actions += exec_judge(&state, &llm, ws_tx, round, phase, &case_summary,
                &participants, &recent_transcript, &[]).await;

            let t = get_recent_transcript(&state, 5).await;
            total_actions += exec_attorney(&state, &llm, ws_tx, round, phase, &case_summary,
                &participants, Party::Defense, &t,
                &def_prior_args, &jury_status).await;

            let t = get_recent_transcript(&state, 4).await;
            total_actions += exec_witness(&state, &llm, ws_tx, round, &case_summary,
                &participants, Party::Defense, &t).await;

            let t = get_recent_transcript(&state, 6).await;
            total_actions += exec_attorney(&state, &llm, ws_tx, round, phase, &case_summary,
                &participants, Party::Prosecution, &t,
                &pros_prior_args, &jury_status).await;
        }

        TrialPhase::Rebuttal => {
            total_actions += exec_judge(&state, &llm, ws_tx, round, phase, &case_summary,
                &participants, &recent_transcript, &[]).await;

            let t = get_recent_transcript(&state, 5).await;
            total_actions += exec_attorney(&state, &llm, ws_tx, round, phase, &case_summary,
                &participants, Party::Prosecution, &t,
                &pros_prior_args, &jury_status).await;

            let t = get_recent_transcript(&state, 6).await;
            total_actions += exec_attorney(&state, &llm, ws_tx, round, phase, &case_summary,
                &participants, Party::Defense, &t,
                &def_prior_args, &jury_status).await;
        }

        TrialPhase::Closing => {
            total_actions += exec_judge(&state, &llm, ws_tx, round, phase, &case_summary,
                &participants, &recent_transcript, &[]).await;

            let t = get_recent_transcript(&state, 5).await;
            total_actions += exec_attorney(&state, &llm, ws_tx, round, phase, &case_summary,
                &participants, Party::Prosecution, &t,
                &pros_prior_args, &jury_status).await;

            let t = get_recent_transcript(&state, 6).await;
            total_actions += exec_attorney(&state, &llm, ws_tx, round, phase, &case_summary,
                &participants, Party::Defense, &t,
                &def_prior_args, &jury_status).await;
        }

        TrialPhase::Deliberation => {
            total_actions += exec_deliberation(&state, &llm, ws_tx, round).await;

            let (_, end) = schedule.phase_range(TrialPhase::Deliberation);
            if round >= end {
                let verdict = {
                    let mut s = state.write().await;
                    let trial = s.trial.as_mut().unwrap();
                    trial.finalize_verdict()
                };
                let _ = ws_tx.send(WsEvent::TrialVerdict {
                    result: format!("{:?}", verdict.verdict).to_lowercase(),
                    guilty: verdict.guilty_votes,
                    not_guilty: verdict.not_guilty_votes,
                    unanimous: verdict.unanimous,
                });
                tracing::info!("VERDICT: {:?} ({}-{})", verdict.verdict, verdict.guilty_votes, verdict.not_guilty_votes);

                // Auto-save completed trial
                let s = state.read().await;
                match crate::trial_store::save_trial(&s) {
                    Ok(id) => tracing::info!("Trial saved with ID: {id}"),
                    Err(e) => tracing::warn!("Failed to save trial: {e}"),
                }
            }
        }
    }

    // 7. Record momentum
    {
        let mut s = state.write().await;
        if let Some(ref mut trial) = s.trial {
            trial.record_momentum(round);
        }
        s.total_actions += total_actions;
    }

    // 8. Broadcast jury update
    broadcast_jury_update(&state, ws_tx, round).await;

    let simulated_time = state.read().await.world.simulated_time;
    let active_agents = state.read().await.agents.len();

    Ok(RoundSummary {
        round,
        simulated_time,
        active_agents,
        total_actions,
        new_posts: total_actions,
        new_replies: 0,
        new_likes: 0,
        new_reposts: 0,
        new_quote_reposts: 0,
        new_follows: 0,
        events_injected: 0,
        new_solutions: 0,
    })
}

// ---------------------------------------------------------------------------
// Participant lookup
// ---------------------------------------------------------------------------

struct CourtParticipants {
    judge_id: Uuid,
    judge_name: String,
    prosecutor_id: Uuid,
    prosecutor_name: String,
    defense_id: Uuid,
    defense_name: String,
    witness_ids: Vec<(Uuid, String, String)>, // id, name, persona
}

async fn get_participants(state: &SharedState) -> CourtParticipants {
    let s = state.read().await;

    let judge = s.agents.iter()
        .find(|(_, p)| matches!(p.tier, Tier::Tier1))
        .map(|(id, p)| (*id, p.name.clone()))
        .unwrap_or((Uuid::nil(), "Judge".into()));

    // First 2 Tier2 agents = attorneys (prosecutor, defense), rest = witnesses
    let mut tier2_agents: Vec<(Uuid, String, String)> = s.agents.iter()
        .filter(|(_, p)| matches!(p.tier, Tier::Tier2))
        .map(|(id, p)| (*id, p.name.clone(), p.persona.clone()))
        .collect();
    tier2_agents.sort_by_key(|(id, _, _)| id.to_string()); // deterministic order

    let prosecutor = tier2_agents.first()
        .map(|(id, name, _)| (*id, name.clone()))
        .unwrap_or((Uuid::nil(), "Prosecutor".into()));
    let defense = tier2_agents.get(1)
        .map(|(id, name, _)| (*id, name.clone()))
        .unwrap_or((Uuid::nil(), "Defense Attorney".into()));

    // Everyone else in Tier2 is a witness
    let witnesses: Vec<(Uuid, String, String)> = tier2_agents.into_iter().skip(2).collect();

    CourtParticipants {
        judge_id: judge.0,
        judge_name: judge.1,
        prosecutor_id: prosecutor.0,
        prosecutor_name: prosecutor.1,
        defense_id: defense.0,
        defense_name: defense.1,
        witness_ids: witnesses,
    }
}

// ---------------------------------------------------------------------------
// Context builders
// ---------------------------------------------------------------------------

async fn get_recent_transcript(state: &SharedState, n: usize) -> String {
    let s = state.read().await;
    s.trial.as_ref()
        .map(|t| {
            t.transcript.iter().rev().take(n).rev()
                .map(|e| format!("{} [{}]: {}", e.speaker_name, e.speaker_role, e.content))
                .collect::<Vec<_>>()
                .join("\n\n")
        })
        .unwrap_or_default()
}

async fn get_prior_arguments(state: &SharedState) -> (Vec<String>, Vec<String>) {
    let s = state.read().await;
    let trial = match &s.trial {
        Some(t) => t,
        None => return (Vec::new(), Vec::new()),
    };

    let mut pros = Vec::new();
    let mut def = Vec::new();

    for e in &trial.transcript {
        let summary: String = e.content.chars().take(100).collect();
        match e.speaker_role {
            CourtRole::Prosecutor => pros.push(summary),
            CourtRole::DefenseAttorney => def.push(summary),
            _ => {}
        }
    }

    // Keep last 5 to avoid prompt bloat
    let pros_last: Vec<String> = pros.into_iter().rev().take(5).collect();
    let def_last: Vec<String> = def.into_iter().rev().take(5).collect();
    (pros_last, def_last)
}

async fn get_jury_status_string(state: &SharedState) -> String {
    let s = state.read().await;
    let trial = match &s.trial {
        Some(t) => t,
        None => return "Unknown".into(),
    };
    let (g, u, i) = trial.jury_split();
    let momentum = trial.current_momentum();
    let direction = if momentum > 0.1 { "leaning prosecution" }
        else if momentum < -0.1 { "leaning defense" }
        else { "undecided" };
    format!("{g} leaning guilty, {u} undecided, {i} leaning innocent ({direction})")
}

// ---------------------------------------------------------------------------
// Execution functions
// ---------------------------------------------------------------------------

async fn exec_judge(
    state: &SharedState,
    llm: &LlmClient,
    ws_tx: &broadcast::Sender<WsEvent>,
    round: u32,
    phase: TrialPhase,
    case_summary: &str,
    participants: &CourtParticipants,
    recent_transcript: &str,
    pending_objections: &[String],
) -> usize {
    let system = build_judge_system_prompt(
        &participants.judge_name,
        case_summary,
        phase,
        recent_transcript,
        pending_objections,
    );
    let user = format!("Round {round}. Proceed.");

    match llm.call_tier(Tier::Tier1, &system, &user).await {
        Ok(raw) => {
            let content = extract_content_for(&raw, &participants.judge_name);
            broadcast_and_record(
                state, ws_tx, round,
                participants.judge_id, &participants.judge_name,
                Tier::Tier1, CourtRole::Judge, &content, Party::Prosecution, // party irrelevant for judge
                false, // no jury impact for judge statements
            ).await;
            1
        }
        Err(e) => { tracing::error!("Judge LLM failed: {e}"); 0 }
    }
}

async fn exec_attorney(
    state: &SharedState,
    llm: &LlmClient,
    ws_tx: &broadcast::Sender<WsEvent>,
    round: u32,
    phase: TrialPhase,
    case_summary: &str,
    participants: &CourtParticipants,
    party: Party,
    recent_transcript: &str,
    prior_args: &[String],
    jury_status: &str,
) -> usize {
    let (id, name) = match party {
        Party::Prosecution => (participants.prosecutor_id, &participants.prosecutor_name),
        Party::Defense => (participants.defense_id, &participants.defense_name),
    };

    // Build witness names list so attorney knows who they can call
    let witness_names: Vec<String> = participants.witness_ids.iter()
        .map(|(_, n, _)| n.clone())
        .collect();
    let witness_list = if witness_names.is_empty() {
        String::new()
    } else {
        format!("\nAVAILABLE WITNESSES: {}\nWhen calling a witness, use their EXACT name.", witness_names.join(", "))
    };

    let system = build_attorney_system_prompt(
        name, party, case_summary, phase,
        &format!("{recent_transcript}{witness_list}"), prior_args, jury_status,
    );
    let user = format!("Round {round}. Your turn.");

    match llm.call_tier(Tier::Tier2, &system, &user).await {
        Ok(raw) => {
            let content = extract_content_for(&raw, name);
            let role = match party {
                Party::Prosecution => CourtRole::Prosecutor,
                Party::Defense => CourtRole::DefenseAttorney,
            };
            broadcast_and_record(
                state, ws_tx, round,
                id, name, Tier::Tier2, role, &content, party, true,
            ).await;

            // Detect objection and have the judge rule
            let is_objection = content.to_uppercase().contains("OBJECTION");
            if is_objection {
                // Judge rules on the objection
                let ruling = exec_judge_ruling(state, llm, ws_tx, round, &content, participants).await;
                return 2; // attorney + judge ruling
            }

            1
        }
        Err(e) => { tracing::error!("Attorney LLM failed: {e}"); 0 }
    }
}

/// Judge rules on an objection — quick LLM call for "sustained" or "overruled"
async fn exec_judge_ruling(
    state: &SharedState,
    llm: &LlmClient,
    ws_tx: &broadcast::Sender<WsEvent>,
    round: u32,
    objection_content: &str,
    participants: &CourtParticipants,
) -> usize {
    let system = format!(
        r#"You are Judge {}, presiding over this trial. An attorney just raised an objection.
Rule on it briefly: either "Sustained." or "Overruled." followed by a one-sentence explanation.
Be decisive and authoritative. 1-2 sentences max.
NEVER prefix with your name."#,
        participants.judge_name
    );
    let user = format!("The attorney objected: \"{}\"\n\nRule on this objection.", objection_content);

    match llm.call_tier(Tier::Tier1, &system, &user).await {
        Ok(raw) => {
            let content = extract_content_for(&raw, &participants.judge_name);
            let sustained = content.to_lowercase().contains("sustained");

            broadcast_and_record(
                state, ws_tx, round,
                participants.judge_id, &participants.judge_name,
                Tier::Tier1, CourtRole::Judge, &content, Party::Prosecution, false,
            ).await;

            // Record the objection
            {
                let mut s = state.write().await;
                if let Some(ref mut trial) = s.trial {
                    trial.record_objection(ObjectionRecord {
                        round,
                        objector: participants.prosecutor_id, // simplified
                        objector_role: Party::Prosecution,
                        grounds: ObjectionGrounds::Relevance,
                        sustained,
                        context: objection_content.chars().take(100).collect(),
                    });
                }
            }

            // Broadcast objection event
            let _ = ws_tx.send(WsEvent::TrialObjection {
                round,
                by_name: "Attorney".into(),
                grounds: "see transcript".into(),
                ruling: if sustained { "sustained".into() } else { "overruled".into() },
            });

            1
        }
        Err(e) => { tracing::warn!("Judge ruling failed: {e}"); 0 }
    }
}

async fn exec_witness(
    state: &SharedState,
    llm: &LlmClient,
    ws_tx: &broadcast::Sender<WsEvent>,
    round: u32,
    case_summary: &str,
    participants: &CourtParticipants,
    called_by: Party,
    recent_transcript: &str,
) -> usize {
    if participants.witness_ids.is_empty() {
        return 0;
    }

    // Extract the last attorney statement as "the question"
    let question = {
        let s = state.read().await;
        s.trial.as_ref()
            .and_then(|t| t.transcript.last())
            .map(|e| e.content.clone())
            .unwrap_or_else(|| "Please state what you know about this case.".into())
    };

    // Try to match witness by name mentioned in the attorney's statement
    let question_lower = question.to_lowercase();
    let witness_idx = participants.witness_ids.iter()
        .position(|(_, name, _)| {
            // Check if the attorney mentioned this witness by name
            let name_lower = name.to_lowercase();
            question_lower.contains(&name_lower)
                || name_lower.split_whitespace().any(|part| {
                    part.len() > 2 && question_lower.contains(part)
                })
        })
        .unwrap_or((round as usize) % participants.witness_ids.len()); // fallback to rotation

    let (witness_id, witness_name, witness_persona) = &participants.witness_ids[witness_idx];

    let nervousness = 0.3 + (round as f32 * 0.02).min(0.4); // gets more nervous over time
    let system = build_witness_system_prompt(
        witness_name,
        WitnessType::Eyewitness,
        called_by,
        witness_persona,
        nervousness,
        recent_transcript,
        &question,
    );
    let user = "Answer the question.".to_string();

    match llm.call_tier(Tier::Tier2, &system, &user).await {
        Ok(raw) => {
            let content = extract_content_for(&raw, witness_name);
            broadcast_and_record(
                state, ws_tx, round,
                *witness_id, witness_name, Tier::Tier2,
                CourtRole::Witness, &content, called_by, true,
            ).await;
            1
        }
        Err(e) => { tracing::error!("Witness LLM failed: {e}"); 0 }
    }
}

async fn exec_deliberation(
    state: &SharedState,
    llm: &LlmClient,
    ws_tx: &broadcast::Sender<WsEvent>,
    round: u32,
) -> usize {
    let juror_data: Vec<(Uuid, String, u8, f32, f32, Vec<KeyMoment>)> = {
        let s = state.read().await;
        let trial = match &s.trial { Some(t) => t, None => return 0 };
        trial.juror_states.iter().map(|(id, js)| {
            let name = s.agents.get(id).map(|a| a.name.clone()).unwrap_or_default();
            (*id, name, js.seat, js.conviction, js.confidence, js.key_moments.clone())
        }).collect()
    };

    let peer_args = get_recent_transcript(state, 6).await;
    let mut total = 0;

    // 3-4 jurors speak per round (rotate by round)
    let speakers: Vec<_> = juror_data.iter()
        .filter(|(_, _, seat, _, _, _)| (*seat as u32 + round) % 3 == 0)
        .take(4)
        .collect();

    for (juror_id, name, seat, conviction, confidence, key_moments) in &speakers {
        let system = build_jury_deliberation_prompt(name, *seat, *conviction, *confidence, key_moments, &[peer_args.clone()]);
        let user = format!("Deliberation round {round}. State your position.");

        match llm.call_tier(Tier::Tier3, &system, &user).await {
            Ok(raw) => {
                let content = extract_content_for(&raw, name);

                let _ = ws_tx.send(WsEvent::Action {
                    data: Action {
                        id: Uuid::new_v4(), round,
                        simulated_time: chrono::Utc::now(),
                        agent_id: *juror_id, agent_name: name.clone(),
                        agent_tier: Tier::Tier3,
                        action_type: ActionType::CreatePost,
                        content: Some(content.clone()),
                        target_post_id: None, target_agent_id: None, reasoning: None,
                    },
                });

                let _ = ws_tx.send(WsEvent::TrialArgument {
                    round,
                    speaker_id: juror_id.to_string(),
                    speaker_name: name.clone(),
                    speaker_role: "juror".into(),
                    content: content.clone(),
                    jury_impact: Vec::new(),
                });

                {
                    let mut s = state.write().await;
                    if let Some(ref mut trial) = s.trial {
                        trial.add_transcript(TranscriptEntry {
                            round, speaker_id: *juror_id, speaker_name: name.clone(),
                            speaker_role: CourtRole::Juror, content, jury_impact: Vec::new(),
                        });
                    }
                }
                total += 1;
            }
            Err(e) => tracing::warn!("Juror #{seat} failed: {e}"),
        }
    }

    // Peer pressure
    {
        let mut s = state.write().await;
        if let Some(ref mut trial) = s.trial {
            let avg = trial.avg_conviction();
            for (_, js) in trial.juror_states.iter_mut() {
                js.apply_peer_pressure(avg, round);
            }
        }
    }

    total
}

// ---------------------------------------------------------------------------
// Shared: broadcast action + record transcript + apply jury impact
// ---------------------------------------------------------------------------

async fn broadcast_and_record(
    state: &SharedState,
    ws_tx: &broadcast::Sender<WsEvent>,
    round: u32,
    agent_id: Uuid,
    agent_name: &str,
    tier: Tier,
    role: CourtRole,
    content: &str,
    party: Party,
    apply_jury: bool,
) {
    // Broadcast standard action
    let _ = ws_tx.send(WsEvent::Action {
        data: Action {
            id: Uuid::new_v4(), round,
            simulated_time: chrono::Utc::now(),
            agent_id, agent_name: agent_name.to_string(), agent_tier: tier,
            action_type: ActionType::CreatePost,
            content: Some(content.to_string()),
            target_post_id: None, target_agent_id: None, reasoning: None,
        },
    });

    let impact_strength = estimate_argument_strength(content);
    let emotional_weight = estimate_emotional_weight(content);

    let mut ws_jury_impact = Vec::new();

    {
        let mut s = state.write().await;
        // Collect agent names before mutable trial borrow
        let agent_names: HashMap<Uuid, String> = s.agents.iter()
            .map(|(id, a)| (*id, a.name.clone())).collect();

        if let Some(ref mut trial) = s.trial {
            let mut jury_impact = Vec::new();

            if apply_jury {
                for (_, js) in trial.juror_states.iter_mut() {
                    let old = js.conviction;
                    js.apply_argument_impact(
                        round, party, impact_strength, emotional_weight,
                        agent_id, content.chars().take(80).collect(),
                    );
                    js.update_trust(party, impact_strength * 0.04);
                    let delta = js.conviction - old;
                    if delta.abs() > 0.005 {
                        jury_impact.push((js.seat, delta));
                        ws_jury_impact.push((js.seat, delta, js.conviction));
                    }
                }
            }

            trial.add_transcript(TranscriptEntry {
                round,
                speaker_id: agent_id,
                speaker_name: agent_name.to_string(),
                speaker_role: role,
                content: content.to_string(),
                jury_impact,
            });
        }
    }

    // Broadcast trial-specific event
    let _ = ws_tx.send(WsEvent::TrialArgument {
        round,
        speaker_id: agent_id.to_string(),
        speaker_name: agent_name.to_string(),
        speaker_role: format!("{}", role),
        content: content.to_string(),
        jury_impact: ws_jury_impact,
    });
}

async fn broadcast_jury_update(state: &SharedState, ws_tx: &broadcast::Sender<WsEvent>, round: u32) {
    let s = state.read().await;
    let trial = match &s.trial { Some(t) => t, None => return };

    let jurors: Vec<TrialJurorSnapshot> = trial.juror_states.iter()
        .map(|(id, js)| {
            let name = s.agents.get(id).map(|a| a.name.clone()).unwrap_or_default();
            TrialJurorSnapshot {
                seat: js.seat, name,
                conviction: js.conviction,
                confidence: js.confidence,
                conviction_label: js.conviction_label().to_string(),
                trust_prosecution: js.trust_prosecution,
                trust_defense: js.trust_defense,
            }
        })
        .collect();

    let _ = ws_tx.send(WsEvent::TrialJuryUpdate { round, jurors });
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn extract_content_for(raw: &str, speaker_name: &str) -> String {
    let content = extract_content_raw(raw);
    // Strip "Name:" or "Name for the prosecution/defense." prefix
    // LLMs often prefix with the speaker's name
    strip_name_prefix(&content, speaker_name)
}

fn strip_name_prefix(text: &str, name: &str) -> String {
    let trimmed = text.trim();

    // Check for "Full Name:" prefix
    if let Some(rest) = trimmed.strip_prefix(name) {
        let rest = rest.trim_start_matches(':').trim_start_matches(',').trim();
        if !rest.is_empty() {
            return rest.to_string();
        }
    }

    // Check for "First Name:" prefix
    if let Some(first) = name.split_whitespace().next() {
        if let Some(rest) = trimmed.strip_prefix(first) {
            let rest = rest.trim_start_matches(':').trim_start_matches(',').trim();
            if !rest.is_empty() {
                return rest.to_string();
            }
        }
    }

    // Check for "Name for the prosecution/defense." or "Name, for the..." prefix
    let lower = trimmed.to_lowercase();
    let name_lower = name.to_lowercase();
    if lower.starts_with(&name_lower) {
        let after = &trimmed[name.len()..];
        let after = after.trim_start_matches(':')
            .trim_start_matches(',')
            .trim_start_matches(" for the prosecution")
            .trim_start_matches(" for the defense")
            .trim_start_matches('.')
            .trim();
        if !after.is_empty() {
            return after.to_string();
        }
    }

    trimmed.to_string()
}

fn extract_content_raw(raw: &str) -> String {
    // Try to find JSON (possibly wrapped in markdown)
    let json_str = if let Some(start) = raw.find("```") {
        let after = &raw[start + 3..];
        let content_start = after.find('\n').map(|i| i + 1).unwrap_or(0);
        let content = &after[content_start..];
        if let Some(end) = content.find("```") {
            content[..end].trim().to_string()
        } else {
            raw.to_string()
        }
    } else {
        raw.to_string()
    };

    // Try JSON parse
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json_str) {
        // Check actions[0].content first
        if let Some(c) = v.get("actions").and_then(|a| a.get(0)).and_then(|a| a.get("content")).and_then(|c| c.as_str()) {
            return c.to_string();
        }
        // Check actions[0].question (for cross-examination)
        if let Some(q) = v.get("actions").and_then(|a| a.get(0)).and_then(|a| a.get("question")).and_then(|c| c.as_str()) {
            return q.to_string();
        }
        // Check top-level content
        if let Some(c) = v.get("content").and_then(|c| c.as_str()) {
            return c.to_string();
        }
        // Try to find ANY string value in the first action
        if let Some(action) = v.get("actions").and_then(|a| a.get(0)) {
            if let Some(obj) = action.as_object() {
                for (key, val) in obj {
                    if key != "action_type" {
                        if let Some(s) = val.as_str() {
                            if s.len() > 20 {
                                return s.to_string();
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback: strip JSON artifacts and return readable text
    let cleaned = raw.trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    // If it still looks like JSON, try one more parse
    if cleaned.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(cleaned) {
            if let Some(c) = v.get("actions").and_then(|a| a.get(0)).and_then(|a| {
                a.get("content").or_else(|| a.get("question"))
            }).and_then(|c| c.as_str()) {
                return c.to_string();
            }
        }
    }

    cleaned.to_string()
}

fn estimate_argument_strength(content: &str) -> f32 {
    let lower = content.to_lowercase();
    let mut score = 0.35_f32;
    if lower.contains("evidence") || lower.contains("exhibit") { score += 0.15; }
    if lower.contains("testimony") || lower.contains("testified") { score += 0.1; }
    if lower.contains("therefore") || lower.contains("because") || lower.contains("consequently") { score += 0.1; }
    if lower.contains("beyond reasonable doubt") || lower.contains("reasonable doubt") { score += 0.15; }
    if lower.contains("clearly") || lower.contains("undeniable") || lower.contains("proven") { score += 0.1; }
    if lower.contains("slack message") || lower.contains("make the numbers work") { score += 0.15; }
    if lower.contains("audit") || lower.contains("340%") { score += 0.1; }
    if lower.contains("maybe") || lower.contains("perhaps") { score -= 0.1; }
    // Short statements (procedural) have low impact
    if content.len() < 80 { score -= 0.15; }
    score.clamp(0.1, 0.95)
}

fn estimate_emotional_weight(content: &str) -> f32 {
    let lower = content.to_lowercase();
    let mut score = 0.15_f32;
    if lower.contains("victim") || lower.contains("suffering") || lower.contains("family") { score += 0.2; }
    if lower.contains("trust") || lower.contains("betrayal") || lower.contains("betrayed") { score += 0.2; }
    if lower.contains("greed") || lower.contains("devastating") || lower.contains("heartbreaking") { score += 0.2; }
    if lower.contains("fear") || lower.contains("pressure") || lower.contains("intimidat") { score += 0.15; }
    if lower.contains("courage") || lower.contains("whistleblower") { score += 0.15; }
    if content.contains('!') { score += 0.1; }
    if lower.contains("statute") || lower.contains("pursuant") { score -= 0.1; }
    if content.len() < 80 { score -= 0.1; }
    score.clamp(0.0, 1.0)
}
