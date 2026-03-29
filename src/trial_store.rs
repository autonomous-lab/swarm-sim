//! Trial persistence — save completed trials to disk, list and retrieve them.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::engine::SimulationState;
use crate::trial::{TranscriptEntry, VerdictResult, ObjectionRecord, Evidence};

const TRIALS_DIR: &str = "./data/trials";

/// Saved trial summary (for listing).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrialSummary {
    pub id: String,
    pub case_title: String,
    pub scenario: String,
    pub verdict: Option<VerdictResult>,
    pub total_rounds: u32,
    pub juror_count: usize,
    pub timestamp: String,
}

/// Full saved trial (for replay).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedTrial {
    pub id: String,
    pub case_title: String,
    pub scenario: String,
    pub verdict: Option<VerdictResult>,
    pub total_rounds: u32,
    pub timestamp: String,

    // Participants
    pub judge: ParticipantInfo,
    pub prosecutor: ParticipantInfo,
    pub defense: ParticipantInfo,
    pub witnesses: Vec<ParticipantInfo>,
    pub jurors: Vec<JurorInfo>,

    // Full transcript for replay
    pub transcript: Vec<TranscriptEntry>,
    pub objections: Vec<ObjectionRecord>,
    pub evidence: Vec<Evidence>,
    pub momentum: Vec<(u32, f32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantInfo {
    pub id: String,
    pub name: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JurorInfo {
    pub id: String,
    pub name: String,
    pub seat: u8,
    pub final_conviction: f32,
    pub final_vote: Option<String>,
    pub conviction_history: Vec<(u32, f32, f32)>,
}

/// Save a completed trial to disk. Returns the trial ID.
pub fn save_trial(state: &SimulationState) -> anyhow::Result<String> {
    let trial = state.trial.as_ref().ok_or_else(|| anyhow::anyhow!("No trial state"))?;

    let id = Uuid::new_v4().to_string()[..8].to_string();
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // Extract case title (first sentence of scenario)
    let scenario = &state.config.simulation.scenario_prompt;
    let case_title = scenario
        .lines()
        .next()
        .unwrap_or("Unknown Case")
        .trim()
        .trim_start_matches("[TRIAL SIMULATION] ")
        .chars()
        .take(80)
        .collect::<String>();

    // Participants
    let judge = state.agents.values()
        .find(|a| matches!(a.tier, crate::agent::Tier::Tier1))
        .map(|a| ParticipantInfo { id: a.id.to_string(), name: a.name.clone(), role: "judge".into() })
        .unwrap_or(ParticipantInfo { id: String::new(), name: "Judge".into(), role: "judge".into() });

    let mut attorneys: Vec<&crate::agent::AgentProfile> = state.agents.values()
        .filter(|a| matches!(a.tier, crate::agent::Tier::Tier2) && a.source_entity.is_none())
        .collect();
    attorneys.sort_by_key(|a| a.id.to_string());

    let prosecutor = attorneys.first()
        .map(|a| ParticipantInfo { id: a.id.to_string(), name: a.name.clone(), role: "prosecutor".into() })
        .unwrap_or(ParticipantInfo { id: String::new(), name: "Prosecutor".into(), role: "prosecutor".into() });

    let defense = attorneys.get(1)
        .map(|a| ParticipantInfo { id: a.id.to_string(), name: a.name.clone(), role: "defense_attorney".into() })
        .unwrap_or(ParticipantInfo { id: String::new(), name: "Defense".into(), role: "defense_attorney".into() });

    let witnesses: Vec<ParticipantInfo> = state.agents.values()
        .filter(|a| matches!(a.tier, crate::agent::Tier::Tier2) && a.source_entity.is_some())
        .map(|a| ParticipantInfo { id: a.id.to_string(), name: a.name.clone(), role: "witness".into() })
        .collect();

    let jurors: Vec<JurorInfo> = trial.juror_states.iter()
        .map(|(agent_id, js)| {
            let name = state.agents.get(agent_id).map(|a| a.name.clone()).unwrap_or_default();
            JurorInfo {
                id: agent_id.to_string(),
                name,
                seat: js.seat,
                final_conviction: js.conviction,
                final_vote: js.vote.map(|v| format!("{:?}", v).to_lowercase()),
                conviction_history: js.conviction_history.clone(),
            }
        })
        .collect();

    let saved = SavedTrial {
        id: id.clone(),
        case_title,
        scenario: scenario.clone(),
        verdict: trial.verdict.clone(),
        total_rounds: state.config.simulation.total_rounds,
        timestamp,
        judge,
        prosecutor,
        defense,
        witnesses,
        jurors,
        transcript: trial.transcript.clone(),
        objections: trial.objection_history.clone(),
        evidence: trial.evidence.clone(),
        momentum: trial.momentum.clone(),
    };

    // Ensure directory exists
    let dir = Path::new(TRIALS_DIR);
    std::fs::create_dir_all(dir)?;

    let path = dir.join(format!("{}.json", id));
    let json = serde_json::to_string_pretty(&saved)?;
    std::fs::write(&path, json)?;

    tracing::info!("Trial saved: {} -> {}", id, path.display());
    Ok(id)
}

/// List all saved trials (summaries only).
pub fn list_trials() -> Vec<TrialSummary> {
    let dir = Path::new(TRIALS_DIR);
    if !dir.exists() {
        return Vec::new();
    }

    let mut trials = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                if let Ok(json) = std::fs::read_to_string(&path) {
                    if let Ok(saved) = serde_json::from_str::<SavedTrial>(&json) {
                        trials.push(TrialSummary {
                            id: saved.id,
                            case_title: saved.case_title,
                            scenario: saved.scenario.chars().take(200).collect(),
                            verdict: saved.verdict,
                            total_rounds: saved.total_rounds,
                            juror_count: saved.jurors.len(),
                            timestamp: saved.timestamp,
                        });
                    }
                }
            }
        }
    }

    // Sort by timestamp descending
    trials.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    trials
}

/// Load a specific trial by ID.
pub fn load_trial(id: &str) -> Option<SavedTrial> {
    // Sanitize ID to prevent path traversal
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        tracing::warn!("Trial load: sanitized reject for id '{}'", id);
        return None;
    }
    let path = Path::new(TRIALS_DIR).join(format!("{}.json", id));
    tracing::info!("Trial load: path={}, exists={}", path.display(), path.exists());
    let json = match std::fs::read_to_string(&path) {
        Ok(j) => j,
        Err(e) => {
            tracing::warn!("Trial load: read failed: {}", e);
            return None;
        }
    };
    match serde_json::from_str::<SavedTrial>(&json) {
        Ok(t) => Some(t),
        Err(e) => {
            tracing::warn!("Trial load: parse failed: {}", e);
            None
        }
    }
}
