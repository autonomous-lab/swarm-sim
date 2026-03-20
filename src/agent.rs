use std::collections::HashMap;

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
    /// Max number of actions per round for this archetype.
    pub fn max_actions(&self) -> usize {
        match self {
            BehaviorArchetype::Lurker => 1,
            BehaviorArchetype::Normie => 2,
            BehaviorArchetype::Shitposter => 2,
            BehaviorArchetype::Cheerleader => 2,
            _ => 3,
        }
    }

    /// Whether this archetype should be blocked from creating original posts.
    pub fn prefers_engagement_only(&self) -> bool {
        matches!(self, BehaviorArchetype::Lurker | BehaviorArchetype::Cheerleader)
    }

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
    /// Country/region for cultural markers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    /// Language style: "formal", "casual", "gen_z", "boomer", "professional", "academic".
    #[serde(default = "default_language_style")]
    pub language_style: String,
}

fn default_language_style() -> String { "casual".to_string() }

impl AgentProfile {
    /// Generate demographic-aware style instructions for prompts.
    pub fn demographic_style(&self) -> String {
        let mut parts = Vec::new();

        // Age-based style
        if let Some(age) = self.age {
            match age {
                0..=22 => parts.push("Gen Z language: 'fr fr', 'no cap', 'lowkey', 'slay', abbreviations, all lowercase.".to_string()),
                23..=35 => parts.push("Millennial tone: casual, uses 'lol', 'ngl', 'tbh'. Internet-native.".to_string()),
                36..=55 => parts.push("Measured, professional-casual. Complete sentences. Occasional emoji.".to_string()),
                _ => parts.push("Formal tone. Proper punctuation. No slang. May misuse a hashtag.".to_string()),
            }
        }

        // Profession-based style
        if let Some(ref prof) = self.profession {
            let lower = prof.to_lowercase();
            if lower.contains("engineer") || lower.contains("developer") || lower.contains("programmer") {
                parts.push("Technical jargon is natural. References to code, systems, architecture.".to_string());
            } else if lower.contains("journalist") || lower.contains("reporter") || lower.contains("writer") {
                parts.push("Clean, concise prose. News framing. Source-citing instinct.".to_string());
            } else if lower.contains("student") || lower.contains("intern") {
                parts.push("Enthusiastic but unsure. Asks questions. Uses 'wait' and '??'.".to_string());
            } else if lower.contains("executive") || lower.contains("ceo") || lower.contains("founder") {
                parts.push("Strategic framing. Bullet points. 'Here'\''s what this means:' style.".to_string());
            }
        }

        // Country/cultural markers
        if let Some(ref country) = self.country {
            let lower = country.to_lowercase();
            if lower.contains("uk") || lower.contains("brit") {
                parts.push("British spelling/phrasing: 'whilst', 'rubbish', dry humour.".to_string());
            } else if lower.contains("india") {
                parts.push("May use 'kindly', 'do the needful' patterns. Formal-casual mix.".to_string());
            } else if lower.contains("brazil") || lower.contains("latam") {
                parts.push("Warm, expressive. May use 'haha' instead of 'lol'.".to_string());
            }
        }

        parts.join(" ")
    }

    /// Suggest a content format for this agent based on archetype and randomness.
    pub fn suggest_format(&self, round: u32) -> &'static str {
        // Use round + agent id hash for deterministic randomness
        let hash = self.id.as_bytes()[0] as u32;
        let roll = (round.wrapping_mul(31).wrapping_add(hash)) % 10;

        match self.archetype {
            BehaviorArchetype::Analyst => match roll {
                0..=3 => "standard",      // 40%: normal post
                4..=5 => "thread_opener",  // 20%: "Here's why X matters: (thread)"
                6..=7 => "question",       // 20%: rhetorical question
                _ => "comparison",          // 20%: "X vs Y" format
            },
            BehaviorArchetype::Journalist => match roll {
                0..=4 => "breaking",       // 50%: "BREAKING:" or "Just in:"
                5..=6 => "question",       // 20%: probing question
                _ => "standard",
            },
            BehaviorArchetype::Shitposter => match roll {
                0..=3 => "standard",
                4..=6 => "meme_text",      // 30%: meme format
                _ => "sarcastic_reply",
            },
            BehaviorArchetype::Normie => match roll {
                0..=5 => "reaction",       // 60%: just a reaction
                _ => "standard",
            },
            BehaviorArchetype::Activist => match roll {
                0..=3 => "call_to_action", // 40%: "We need to..."
                4..=6 => "standard",
                _ => "question",
            },
            _ => "standard",
        }
    }
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
// Cognitive State — fatigue, attention, saturation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveState {
    /// 0.0 = fresh, 1.0 = exhausted. Increases with actions, decays when idle.
    pub fatigue: f32,
    /// How many feed items the agent actually processes (reduced by fatigue).
    /// 1.0 = full attention, 0.0 = ignoring everything.
    pub attention: f32,
    /// Consecutive rounds this agent was active.
    pub rounds_active: u32,
    /// Consecutive rounds this agent was idle.
    pub rounds_idle: u32,
    /// Topics the agent has been exposed to recently (topic -> exposure count).
    /// Used for saturation: repeated exposure to same topic = less engagement.
    #[serde(default)]
    pub topic_saturation: HashMap<String, u32>,
}

impl Default for CognitiveState {
    fn default() -> Self {
        Self {
            fatigue: 0.0,
            attention: 1.0,
            rounds_active: 0,
            rounds_idle: 0,
            topic_saturation: HashMap::new(),
        }
    }
}

impl CognitiveState {
    /// Update after an active round where the agent performed `action_count` actions.
    pub fn on_active_round(&mut self, action_count: u32) {
        self.rounds_active += 1;
        self.rounds_idle = 0;
        // Fatigue increases with actions and consecutive activity
        let action_fatigue = action_count as f32 * 0.08;
        let streak_fatigue = (self.rounds_active as f32 * 0.03).min(0.15);
        self.fatigue = (self.fatigue + action_fatigue + streak_fatigue).min(1.0);
        // Attention decreases with fatigue
        self.attention = (1.0 - self.fatigue * 0.6).max(0.2);
        // Decay old topic saturation
        self.topic_saturation.retain(|_, v| {
            *v = v.saturating_sub(1);
            *v > 0
        });
    }

    /// Update after an idle round (agent didn't act).
    pub fn on_idle_round(&mut self) {
        self.rounds_idle += 1;
        self.rounds_active = 0;
        // Recovery: fatigue decays faster when idle
        let recovery = 0.15 + self.rounds_idle as f32 * 0.05;
        self.fatigue = (self.fatigue - recovery).max(0.0);
        self.attention = (1.0 - self.fatigue * 0.6).max(0.2);
        // Faster topic saturation decay when idle
        self.topic_saturation.clear();
    }

    /// Effective max actions for this round, factoring in fatigue.
    pub fn effective_max_actions(&self, archetype_max: usize) -> usize {
        let factor = 1.0 - self.fatigue * 0.5;
        (archetype_max as f32 * factor).ceil() as usize
    }

    /// Effective feed size, factoring in attention.
    pub fn effective_feed_size(&self, base_feed_size: usize) -> usize {
        (base_feed_size as f32 * self.attention).ceil() as usize
    }

    /// Record exposure to a topic keyword.
    pub fn expose_to_topic(&mut self, topic: &str) {
        *self.topic_saturation.entry(topic.to_lowercase()).or_insert(0) += 1;
    }

    /// How saturated the agent is on a given topic (0.0 = fresh, 1.0 = completely bored).
    pub fn topic_boredom(&self, topic: &str) -> f32 {
        let count = self.topic_saturation.get(&topic.to_lowercase()).copied().unwrap_or(0);
        (count as f32 / 5.0).min(1.0)
    }
}

// ---------------------------------------------------------------------------
// Relational Memory — trust, influence, interaction history between agents
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationalMemory {
    /// Trust toward other agents: -1.0 (distrust) to 1.0 (high trust).
    /// Builds through positive interactions, decays through disagreements.
    #[serde(default)]
    pub trust: HashMap<Uuid, f32>,
    /// Perceived influence of other agents: 0.0 to 1.0.
    /// Increases when their posts get high engagement.
    #[serde(default)]
    pub influence: HashMap<Uuid, f32>,
    /// Interaction count with each agent (likes, replies, etc.)
    #[serde(default)]
    pub interaction_count: HashMap<Uuid, u32>,
    /// Last round of interaction with each agent.
    #[serde(default)]
    pub last_interaction: HashMap<Uuid, u32>,
}

impl Default for RelationalMemory {
    fn default() -> Self {
        Self {
            trust: HashMap::new(),
            influence: HashMap::new(),
            interaction_count: HashMap::new(),
            last_interaction: HashMap::new(),
        }
    }
}

impl RelationalMemory {
    /// Record a positive interaction (like, repost, supportive reply).
    pub fn record_positive(&mut self, other: Uuid, round: u32) {
        let trust = self.trust.entry(other).or_insert(0.0);
        *trust = (*trust + 0.1).min(1.0);
        *self.interaction_count.entry(other).or_insert(0) += 1;
        self.last_interaction.insert(other, round);
    }

    /// Record a negative interaction (disagreement, unfollow).
    pub fn record_negative(&mut self, other: Uuid, round: u32) {
        let trust = self.trust.entry(other).or_insert(0.0);
        *trust = (*trust - 0.15).max(-1.0);
        *self.interaction_count.entry(other).or_insert(0) += 1;
        self.last_interaction.insert(other, round);
    }

    /// Update perceived influence of another agent based on their engagement.
    pub fn update_influence(&mut self, other: Uuid, engagement_score: f64) {
        let influence = self.influence.entry(other).or_insert(0.0);
        let normalized = (engagement_score / 20.0).min(1.0) as f32;
        // Exponential moving average
        *influence = *influence * 0.8 + normalized * 0.2;
    }

    /// Get trust level for another agent (default 0.0 = neutral).
    pub fn trust_for(&self, other: &Uuid) -> f32 {
        self.trust.get(other).copied().unwrap_or(0.0)
    }

    /// Get perceived influence of another agent (default 0.0).
    pub fn influence_of(&self, other: &Uuid) -> f32 {
        self.influence.get(other).copied().unwrap_or(0.0)
    }

    /// Decay trust and influence over time (called each round).
    pub fn decay(&mut self, current_round: u32) {
        // Trust decays slowly toward 0 for stale relationships
        let last_interaction = &self.last_interaction;
        self.trust.retain(|id, trust| {
            let rounds_since = current_round.saturating_sub(
                last_interaction.get(id).copied().unwrap_or(0)
            );
            if rounds_since > 5 {
                *trust *= 0.95; // slow decay
            }
            trust.abs() > 0.01 // remove negligible entries
        });
        // Influence decays faster
        self.influence.retain(|_, inf| {
            *inf *= 0.9;
            *inf > 0.01
        });
    }

    /// Render for prompt injection (compact).
    pub fn render_short(&self) -> String {
        if self.trust.is_empty() {
            return String::new();
        }
        let mut parts = Vec::new();
        // Only show significant relationships
        let mut sorted: Vec<_> = self.trust.iter()
            .filter(|(_, t)| t.abs() > 0.2)
            .collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
        for (id, trust) in sorted.iter().take(5) {
            let label = if **trust > 0.5 { "ally" }
                else if **trust > 0.2 { "friendly" }
                else if **trust < -0.5 { "rival" }
                else { "tense" };
            parts.push(format!("{}:{}", &id.to_string()[..8], label));
        }
        if parts.is_empty() { String::new() } else { format!("Relations: {}", parts.join(", ")) }
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
    #[serde(default)]
    pub pending_notifications: Vec<String>,
    #[serde(default)]
    pub cognitive: CognitiveState,
    #[serde(default)]
    pub relations: RelationalMemory,
    /// Per-topic beliefs: topic keyword -> conviction (-1.0 opposing to 1.0 supportive).
    /// Evolves through exposure and trusted sources.
    #[serde(default)]
    pub beliefs: HashMap<String, f32>,
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
            pending_notifications: Vec::new(),
            cognitive: CognitiveState::default(),
            relations: RelationalMemory::default(),
            beliefs: HashMap::new(),
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
            pending_notifications: Vec::new(),
            cognitive: CognitiveState::default(),
            relations: RelationalMemory::default(),
            beliefs: HashMap::new(),
        }
    }

    pub fn log_action(&mut self, entry: ActionLogEntry) {
        if self.action_log.len() >= MAX_ACTION_LOG {
            self.action_log.remove(0);
        }
        self.action_log.push(entry);
    }

    /// Update beliefs based on exposure to content with a given sentiment on a topic.
    /// `source_trust` modulates how much the agent is influenced (trusted source = more influence).
    pub fn update_belief(&mut self, topic: &str, content_sentiment: f32, source_trust: f32) {
        let topic_key = topic.to_lowercase();
        let current = self.beliefs.get(&topic_key).copied().unwrap_or(0.0);

        // Resistance: stronger existing belief = harder to change
        let resistance = current.abs() * 0.3;
        // Trust factor: trusted sources influence more
        let trust_factor = (0.3 + source_trust * 0.7).max(0.05);
        // Learning rate: how fast beliefs shift
        let lr = 0.08 * trust_factor * (1.0 - resistance);

        let pull = content_sentiment - current;
        let new_belief = (current + pull * lr).clamp(-1.0, 1.0);
        self.beliefs.insert(topic_key, new_belief);
    }

    /// Render belief summary for prompt injection.
    pub fn beliefs_summary(&self) -> String {
        if self.beliefs.is_empty() {
            return String::new();
        }
        let mut sorted: Vec<_> = self.beliefs.iter()
            .filter(|(_, v)| v.abs() > 0.15)
            .collect();
        sorted.sort_by(|a, b| b.1.abs().partial_cmp(&a.1.abs()).unwrap_or(std::cmp::Ordering::Equal));
        let parts: Vec<String> = sorted.iter().take(5)
            .map(|(topic, &strength)| {
                let stance = if strength > 0.4 { "strongly for" }
                    else if strength > 0.15 { "leaning for" }
                    else if strength < -0.4 { "strongly against" }
                    else { "leaning against" };
                format!("{}: {}", topic, stance)
            })
            .collect();
        if parts.is_empty() { String::new() } else { format!("Your beliefs: {}", parts.join(", ")) }
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
