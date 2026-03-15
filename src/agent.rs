use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Tier1,
    Tier2,
    Tier3,
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tier::Tier1 => write!(f, "tier1"),
            Tier::Tier2 => write!(f, "tier2"),
            Tier::Tier3 => write!(f, "tier3"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stance {
    Supportive,
    Opposing,
    Neutral,
    Observer,
}

impl std::fmt::Display for Stance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Stance::Supportive => write!(f, "supportive"),
            Stance::Opposing => write!(f, "opposing"),
            Stance::Neutral => write!(f, "neutral"),
            Stance::Observer => write!(f, "observer"),
        }
    }
}

// ---------------------------------------------------------------------------
// Agent Profile (immutable during simulation)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub id: Uuid,
    pub name: String,
    pub username: String,
    pub tier: Tier,

    // Personality
    pub bio: String,
    pub persona: String,
    pub stance: Stance,
    pub sentiment_bias: f32,
    pub influence_weight: f32,

    // Activity
    pub activity_level: f32,
    pub active_hours: Vec<u8>,

    // Demographics
    pub interests: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profession: Option<String>,

    // Source entity (from extraction)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_entity: Option<String>,
}

impl AgentProfile {
    /// Truncate persona for batch prompts.
    pub fn persona_truncated(&self, max_chars: usize) -> &str {
        if self.persona.len() <= max_chars {
            &self.persona
        } else {
            let boundary = self
                .persona
                .char_indices()
                .take_while(|(i, _)| *i < max_chars)
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(max_chars);
            &self.persona[..boundary]
        }
    }
}

// ---------------------------------------------------------------------------
// Agent State (mutable during simulation)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub agent_id: Uuid,
    pub followers: Vec<Uuid>,
    pub following: Vec<Uuid>,
    pub post_ids: Vec<Uuid>,
    pub liked_post_ids: Vec<Uuid>,
    pub memory: AgentMemory,
}

impl AgentState {
    pub fn new(agent_id: Uuid) -> Self {
        Self {
            agent_id,
            followers: Vec::new(),
            following: Vec::new(),
            post_ids: Vec::new(),
            liked_post_ids: Vec::new(),
            memory: AgentMemory::new(20, 5),
        }
    }
}

// ---------------------------------------------------------------------------
// Agent Memory
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMemory {
    /// (round_number, observation_text) — rolling window
    pub recent: Vec<(u32, String)>,
    pub recent_capacity: usize,
    /// Key memories that survive rotation
    pub pinned: Vec<String>,
    pub pinned_capacity: usize,
}

impl AgentMemory {
    pub fn new(recent_capacity: usize, pinned_capacity: usize) -> Self {
        Self {
            recent: Vec::with_capacity(recent_capacity),
            recent_capacity,
            pinned: Vec::new(),
            pinned_capacity,
        }
    }

    pub fn observe(&mut self, round: u32, observation: String) {
        if self.recent.len() >= self.recent_capacity {
            self.recent.remove(0);
        }
        self.recent.push((round, observation));
    }

    pub fn pin(&mut self, memory: String) {
        if self.pinned.len() >= self.pinned_capacity {
            self.pinned.remove(0);
        }
        self.pinned.push(memory);
    }

    /// Render memory as text block for LLM prompt injection.
    pub fn render(&self, current_round: u32) -> String {
        let mut parts = Vec::new();

        if !self.pinned.is_empty() {
            parts.push("KEY MEMORIES:".to_string());
            for m in &self.pinned {
                parts.push(format!("  - {m}"));
            }
        }

        if !self.recent.is_empty() {
            parts.push(format!(
                "\nRECENT ({} observations):",
                self.recent.len()
            ));
            for (round, obs) in &self.recent {
                let ago = current_round.saturating_sub(*round);
                parts.push(format!("  [{ago}r ago] {obs}"));
            }
        }

        parts.join("\n")
    }

    /// Abbreviated render for batched agents (last N only).
    pub fn render_short(&self, current_round: u32, max_recent: usize) -> String {
        let mut parts = Vec::new();

        if !self.pinned.is_empty() {
            if let Some(last) = self.pinned.last() {
                parts.push(format!("KEY: {last}"));
            }
        }

        let start = self.recent.len().saturating_sub(max_recent);
        for (round, obs) in &self.recent[start..] {
            let ago = current_round.saturating_sub(*round);
            parts.push(format!("[{ago}r ago] {obs}"));
        }

        parts.join(" | ")
    }
}
