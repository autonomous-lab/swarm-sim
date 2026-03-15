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

impl Stance {
    /// Derive stance from a sentiment float (-1.0 to 1.0).
    pub fn from_sentiment(s: f32) -> Self {
        if s > 0.3 {
            Stance::Supportive
        } else if s < -0.3 {
            Stance::Opposing
        } else if s.abs() < 0.1 {
            Stance::Observer
        } else {
            Stance::Neutral
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorArchetype {
    Analyst,
    Provocateur,
    Lurker,
    Cheerleader,
    Shitposter,
    Journalist,
    Normie,
    Activist,
}

impl std::fmt::Display for BehaviorArchetype {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BehaviorArchetype::Analyst => write!(f, "analyst"),
            BehaviorArchetype::Provocateur => write!(f, "provocateur"),
            BehaviorArchetype::Lurker => write!(f, "lurker"),
            BehaviorArchetype::Cheerleader => write!(f, "cheerleader"),
            BehaviorArchetype::Shitposter => write!(f, "shitposter"),
            BehaviorArchetype::Journalist => write!(f, "journalist"),
            BehaviorArchetype::Normie => write!(f, "normie"),
            BehaviorArchetype::Activist => write!(f, "activist"),
        }
    }
}

impl BehaviorArchetype {
    /// Max post length in characters for this archetype.
    pub fn max_post_length(&self) -> u16 {
        match self {
            BehaviorArchetype::Analyst => 500,
            BehaviorArchetype::Journalist => 400,
            BehaviorArchetype::Activist => 300,
            BehaviorArchetype::Provocateur => 200,
            BehaviorArchetype::Cheerleader => 150,
            BehaviorArchetype::Normie => 100,
            BehaviorArchetype::Shitposter => 80,
            BehaviorArchetype::Lurker => 50,
        }
    }

    /// Returns (behavior_description, action_preferences, style_rules)
    pub fn prompt_instructions(&self) -> (&'static str, &'static str, &'static str) {
        match self {
            BehaviorArchetype::Analyst => (
                "Thoughtful analyst. Data-driven, measured, cites specifics.",
                "40% create_post, 30% reply, 20% like, 10% follow",
                "2-4 sentences. Reference numbers, comparisons, trends. Measured tone. NO hashtags. Write like a financial analyst quoting.",
            ),
            BehaviorArchetype::Provocateur => (
                "Confrontational contrarian. Pokes holes, rhetorical questions. NEVER agrees politely.",
                "50% reply (DISAGREE), 30% create_post (hot take), 20% like",
                "1-2 sentences MAX. Aggressive, direct, rhetorical questions. Pick fights. Be blunt and dismissive. NO hashtags.",
            ),
            BehaviorArchetype::Lurker => (
                "SILENT OBSERVER. Almost NEVER posts. 1-5 words MAX. Mostly likes and reposts.",
                "60% like, 20% repost, 10% do_nothing, 10% reply (1-5 WORDS ONLY)",
                "STRICT: 1-5 words MAXIMUM. One reaction word or emoji-like expression. NEVER write a full sentence. NO hashtags.",
            ),
            BehaviorArchetype::Cheerleader => (
                "Enthusiastic supporter. Amplifies, cheers, exclamation marks. Short and energetic.",
                "40% like, 25% repost, 20% reply (supportive), 15% follow",
                "1-2 sentences MAX. Exclamation marks, positive energy. NO hashtags. Just raw enthusiasm.",
            ),
            BehaviorArchetype::Shitposter => (
                "Chaotic shitposter. Sarcasm, absurdist humor, irony. Mocks everything including serious takes.",
                "40% create_post (sarcastic), 30% reply (mocking), 20% like, 10% repost",
                "1 sentence MAX. Pure sarcasm, irony, absurd comparisons. Mock serious people. NO hashtags. Think deadpan Twitter humor.",
            ),
            BehaviorArchetype::Journalist => (
                "Reporter/journalist. Probing questions, news framing, seeks angles. Neutral but sharp.",
                "40% create_post (news), 30% reply (questions), 20% like, 10% follow",
                "2-3 sentences. Frame as breaking news. Ask probing questions. Professional tone. NO hashtags (journalists don't use them in posts).",
            ),
            BehaviorArchetype::Normie => (
                "Average person. Simple genuine reactions. NOT analytical. Short casual language.",
                "35% like, 25% reply (short casual), 20% do_nothing, 15% repost, 5% create_post",
                "1-2 sentences MAX. Casual language, contractions, abbreviations (lol, tbh, ngl, idk, bruh, omg, lowkey). React emotionally, not intellectually. NO hashtags. NO analysis.",
            ),
            BehaviorArchetype::Activist => (
                "Passionate activist. Justice lens, systemic framing, calls to action. Urgent and moral.",
                "40% create_post (calls to action), 30% reply (reframe), 20% repost, 10% like",
                "2-3 sentences. Urgent tone. MAX 1 hashtag per post (organic, not generic). Frame as systemic issue. Each post must make a UNIQUE point — never repeat another user's argument.",
            ),
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
    pub archetype: BehaviorArchetype,

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
// Action log entry (lightweight version for agent history)
// ---------------------------------------------------------------------------

const MAX_ACTION_LOG: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionLogEntry {
    pub round: u32,
    pub action_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
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
    pub action_log: Vec<ActionLogEntry>,
    pub current_sentiment: f32,
    pub sentiment_history: Vec<(u32, f32)>,
}

impl AgentState {
    pub fn new(agent_id: Uuid) -> Self {
        Self {
            agent_id,
            followers: Vec::new(),
            following: Vec::new(),
            post_ids: Vec::new(),
            liked_post_ids: Vec::new(),
            memory: AgentMemory::new(30, 8),
            action_log: Vec::new(),
            current_sentiment: 0.0,
            sentiment_history: Vec::new(),
        }
    }

    pub fn new_with_sentiment(agent_id: Uuid, initial_sentiment: f32) -> Self {
        Self {
            agent_id,
            followers: Vec::new(),
            following: Vec::new(),
            post_ids: Vec::new(),
            liked_post_ids: Vec::new(),
            memory: AgentMemory::new(30, 8),
            action_log: Vec::new(),
            current_sentiment: initial_sentiment,
            sentiment_history: vec![(0, initial_sentiment)],
        }
    }

    pub fn log_action(&mut self, entry: ActionLogEntry) {
        if self.action_log.len() >= MAX_ACTION_LOG {
            self.action_log.remove(0);
        }
        self.action_log.push(entry);
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
