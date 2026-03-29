use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Court roles & types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CourtRole {
    Judge,
    Prosecutor,
    DefenseAttorney,
    Witness,
    Juror,
}

impl std::fmt::Display for CourtRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CourtRole::Judge => write!(f, "judge"),
            CourtRole::Prosecutor => write!(f, "prosecutor"),
            CourtRole::DefenseAttorney => write!(f, "defense_attorney"),
            CourtRole::Witness => write!(f, "witness"),
            CourtRole::Juror => write!(f, "juror"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WitnessType {
    Expert,
    Eyewitness,
    Character,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Party {
    Prosecution,
    Defense,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Guilty,
    NotGuilty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectionGrounds {
    Hearsay,
    Relevance,
    LeadingQuestion,
    Speculation,
    AskedAndAnswered,
    Badgering,
}

impl std::fmt::Display for ObjectionGrounds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObjectionGrounds::Hearsay => write!(f, "hearsay"),
            ObjectionGrounds::Relevance => write!(f, "relevance"),
            ObjectionGrounds::LeadingQuestion => write!(f, "leading question"),
            ObjectionGrounds::Speculation => write!(f, "speculation"),
            ObjectionGrounds::AskedAndAnswered => write!(f, "asked and answered"),
            ObjectionGrounds::Badgering => write!(f, "badgering the witness"),
        }
    }
}

// ---------------------------------------------------------------------------
// Trial phases
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrialPhase {
    Opening,
    ProsecutionCase,
    DefenseCase,
    Rebuttal,
    Closing,
    Deliberation,
}

impl std::fmt::Display for TrialPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrialPhase::Opening => write!(f, "Opening Statements"),
            TrialPhase::ProsecutionCase => write!(f, "Prosecution Case"),
            TrialPhase::DefenseCase => write!(f, "Defense Case"),
            TrialPhase::Rebuttal => write!(f, "Rebuttal"),
            TrialPhase::Closing => write!(f, "Closing Arguments"),
            TrialPhase::Deliberation => write!(f, "Jury Deliberation"),
        }
    }
}

impl TrialPhase {
    /// Which roles are active speakers in this phase.
    pub fn active_speakers(&self) -> Vec<CourtRole> {
        match self {
            TrialPhase::Opening => vec![CourtRole::Judge, CourtRole::Prosecutor, CourtRole::DefenseAttorney],
            TrialPhase::ProsecutionCase => vec![CourtRole::Judge, CourtRole::Prosecutor, CourtRole::DefenseAttorney, CourtRole::Witness],
            TrialPhase::DefenseCase => vec![CourtRole::Judge, CourtRole::Prosecutor, CourtRole::DefenseAttorney, CourtRole::Witness],
            TrialPhase::Rebuttal => vec![CourtRole::Judge, CourtRole::Prosecutor, CourtRole::Witness],
            TrialPhase::Closing => vec![CourtRole::Judge, CourtRole::Prosecutor, CourtRole::DefenseAttorney],
            TrialPhase::Deliberation => vec![CourtRole::Juror],
        }
    }

    /// Can jurors speak in this phase?
    pub fn jury_can_speak(&self) -> bool {
        matches!(self, TrialPhase::Deliberation)
    }

    /// Which party leads this phase (for directing examination).
    pub fn leading_party(&self) -> Option<Party> {
        match self {
            TrialPhase::ProsecutionCase | TrialPhase::Rebuttal => Some(Party::Prosecution),
            TrialPhase::DefenseCase => Some(Party::Defense),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Trial phase schedule
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrialSchedule {
    pub phases: Vec<(TrialPhase, u32)>, // (phase, number_of_rounds)
}

impl Default for TrialSchedule {
    fn default() -> Self {
        Self {
            phases: vec![
                (TrialPhase::Opening, 2),
                (TrialPhase::ProsecutionCase, 8),
                (TrialPhase::DefenseCase, 8),
                (TrialPhase::Rebuttal, 2),
                (TrialPhase::Closing, 2),
                (TrialPhase::Deliberation, 5),
            ],
        }
    }
}

impl TrialSchedule {
    pub fn total_rounds(&self) -> u32 {
        self.phases.iter().map(|(_, r)| r).sum()
    }

    /// Get the phase for a given round (1-indexed).
    pub fn phase_for_round(&self, round: u32) -> TrialPhase {
        let mut cumulative = 0;
        for (phase, rounds) in &self.phases {
            cumulative += rounds;
            if round <= cumulative {
                return *phase;
            }
        }
        TrialPhase::Deliberation // fallback
    }

    /// Get the round range for the current phase (start, end) 1-indexed.
    pub fn phase_range(&self, target_phase: TrialPhase) -> (u32, u32) {
        let mut start = 1;
        for (phase, rounds) in &self.phases {
            if *phase == target_phase {
                return (start, start + rounds - 1);
            }
            start += rounds;
        }
        (start, start) // fallback
    }

    /// Progress within current phase (0.0 to 1.0).
    pub fn phase_progress(&self, round: u32) -> f32 {
        let phase = self.phase_for_round(round);
        let (start, end) = self.phase_range(phase);
        let total = (end - start + 1) as f32;
        let progress = (round - start + 1) as f32;
        (progress / total).min(1.0)
    }
}

// ---------------------------------------------------------------------------
// Juror cognitive state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JurorState {
    /// -1.0 (fully innocent) to 1.0 (fully guilty)
    pub conviction: f32,
    /// 0.0 to 1.0 — how certain they are
    pub confidence: f32,
    /// -1.0 to 1.0 — current emotional state
    pub emotional_state: f32,
    /// Trust toward prosecution (0.0 to 1.0)
    pub trust_prosecution: f32,
    /// Trust toward defense (0.0 to 1.0)
    pub trust_defense: f32,
    /// Key moments that shifted conviction
    pub key_moments: Vec<KeyMoment>,
    /// Active cognitive biases
    pub biases: CognitiveBiases,
    /// Final vote (set during deliberation)
    pub vote: Option<Verdict>,
    /// Conviction history per round: (round, conviction, confidence)
    pub conviction_history: Vec<(u32, f32, f32)>,
    /// Seat number (1-12)
    pub seat: u8,
}

impl JurorState {
    pub fn new(seat: u8) -> Self {
        // Give jurors varied initial leanings and trust levels
        // so the jury isn't a monolithic undecided block
        let s = seat as f32;
        let initial_conviction = match seat % 4 {
            0 => 0.15 + (s * 0.03) % 0.1,   // slight prosecution lean
            1 => -0.1 - (s * 0.02) % 0.1,    // slight defense lean
            2 => 0.05,                         // barely leaning prosecution
            _ => -0.05,                        // barely leaning defense
        };
        let initial_trust_pros = 0.4 + (s * 0.07) % 0.3;  // 0.4 to 0.7
        let initial_trust_def = 0.4 + (s * 0.11) % 0.3;   // 0.4 to 0.7

        Self {
            conviction: initial_conviction,
            confidence: 0.15 + (s * 0.03) % 0.15,
            emotional_state: 0.0,
            trust_prosecution: initial_trust_pros,
            trust_defense: initial_trust_def,
            key_moments: Vec::new(),
            biases: CognitiveBiases::random(seat),
            vote: None,
            conviction_history: vec![(0, initial_conviction, 0.2)],
            seat,
        }
    }

    /// Apply the impact of an argument on this juror.
    pub fn apply_argument_impact(
        &mut self,
        round: u32,
        source: Party,
        impact_strength: f32,    // 0.0 to 1.0
        emotional_weight: f32,   // 0.0 to 1.0
        source_agent_id: Uuid,
        content_summary: String,
    ) {
        let trust = match source {
            Party::Prosecution => self.trust_prosecution,
            Party::Defense => self.trust_defense,
        };

        // Direction: prosecution pushes toward guilty (+), defense toward innocent (-)
        let direction = match source {
            Party::Prosecution => 1.0,
            Party::Defense => -1.0,
        };

        // Base impact modulated by trust and confidence resistance
        let resistance = self.confidence * 0.2;
        let effective_impact = impact_strength * (0.5 + trust * 0.5) * (1.0 - resistance);

        // Apply cognitive biases — these vary SIGNIFICANTLY per juror
        let bias_multiplier = self.biases.compute_multiplier(round, emotional_weight);

        // Confirmation bias: arguments that align with existing lean are amplified,
        // arguments against are dampened
        let alignment = direction * self.conviction; // positive if argument aligns with lean
        let confirmation_factor = if alignment > 0.0 {
            1.0 + self.biases.confirmation * 0.6 // confirming = boosted
        } else if self.conviction.abs() > 0.3 {
            1.0 - self.biases.confirmation * 0.4 // contradicting strong belief = dampened
        } else {
            1.0 // undecided jurors are open
        };

        let delta = direction * effective_impact * bias_multiplier * confirmation_factor * 0.3;
        let old_conviction = self.conviction;
        self.conviction = (self.conviction + delta).clamp(-1.0, 1.0);

        // Update confidence: big shifts reduce confidence, confirming arguments increase it
        let shift_magnitude = (self.conviction - old_conviction).abs();
        if shift_magnitude > 0.08 {
            self.confidence = (self.confidence - 0.03).max(0.1);
        } else if (self.conviction - old_conviction).signum() == old_conviction.signum() || old_conviction.abs() < 0.1 {
            // Argument reinforces existing belief
            self.confidence = (self.confidence + 0.04).min(0.95);
        }

        // Update emotional state
        self.emotional_state = (self.emotional_state * 0.7 + emotional_weight * direction * 0.3)
            .clamp(-1.0, 1.0);

        // Record key moment if notable shift
        if shift_magnitude > 0.02 {
            self.key_moments.push(KeyMoment {
                round,
                source_agent: source_agent_id,
                content_summary,
                conviction_delta: self.conviction - old_conviction,
            });
            // Keep only top 10 most impactful moments
            self.key_moments.sort_by(|a, b| {
                b.conviction_delta.abs().partial_cmp(&a.conviction_delta.abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            self.key_moments.truncate(10);
        }

        // Record history
        self.conviction_history.push((round, self.conviction, self.confidence));
    }

    /// Update trust toward a party based on argument quality.
    pub fn update_trust(&mut self, party: Party, delta: f32) {
        // Amplify trust changes so they accumulate meaningfully
        let amplified = delta * 1.5;
        match party {
            Party::Prosecution => {
                self.trust_prosecution = (self.trust_prosecution + amplified).clamp(0.1, 0.95);
            }
            Party::Defense => {
                self.trust_defense = (self.trust_defense + amplified).clamp(0.1, 0.95);
            }
        }
    }

    /// Apply peer pressure during deliberation.
    pub fn apply_peer_pressure(&mut self, avg_conviction: f32, round: u32) {
        let peer_pull = (avg_conviction - self.conviction) * 0.12;
        // Confident jurors resist more, but even they feel group pressure
        let effective_pull = peer_pull * (1.0 - self.confidence * 0.4);
        self.conviction = (self.conviction + effective_pull).clamp(-1.0, 1.0);
        // Peer pressure also slowly increases confidence (conformity)
        self.confidence = (self.confidence + 0.02).min(0.95);
        self.conviction_history.push((round, self.conviction, self.confidence));
    }

    /// Cast final vote based on current conviction.
    pub fn cast_vote(&mut self) -> Verdict {
        let verdict = if self.conviction > 0.0 {
            Verdict::Guilty
        } else {
            Verdict::NotGuilty
        };
        self.vote = Some(verdict);
        verdict
    }

    /// Label for display: "Leaning guilty", "Strongly innocent", etc.
    pub fn conviction_label(&self) -> &'static str {
        match self.conviction {
            c if c > 0.6 => "Strongly guilty",
            c if c > 0.2 => "Leaning guilty",
            c if c > -0.2 => "Undecided",
            c if c > -0.6 => "Leaning innocent",
            _ => "Strongly innocent",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyMoment {
    pub round: u32,
    pub source_agent: Uuid,
    pub content_summary: String,
    pub conviction_delta: f32,
}

// ---------------------------------------------------------------------------
// Cognitive biases
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveBiases {
    /// First impression over-weighted
    pub anchoring: f32,
    /// Recent arguments over-weighted
    pub recency: f32,
    /// Expert testimony over-weighted
    pub authority: f32,
    /// Emotional testimony over-weighted
    pub sympathy: f32,
    /// Confirms existing belief over-weighted
    pub confirmation: f32,
}

impl CognitiveBiases {
    /// Generate varied biases from seat number. Each juror is genuinely different.
    pub fn random(seat: u8) -> Self {
        // Use prime-based hashing for better distribution
        let s = seat as f32;
        let hash = |prime: f32| ((s * prime).sin().abs());
        Self {
            anchoring: 0.1 + hash(7.31) * 0.8,        // 0.1 to 0.9
            recency: 0.1 + hash(13.17) * 0.8,
            authority: 0.1 + hash(17.43) * 0.8,
            sympathy: 0.1 + hash(23.71) * 0.8,
            confirmation: 0.1 + hash(29.53) * 0.7,
        }
    }

    /// Compute a multiplier for argument impact based on biases and context.
    /// Returns a value that varies SIGNIFICANTLY per juror (0.4 to 2.5).
    pub fn compute_multiplier(&self, round: u32, emotional_weight: f32) -> f32 {
        // Base varies per juror: some are naturally more persuadable
        let mut mult = 0.6 + self.authority * 0.4; // 0.6 to 1.4

        // Anchoring: first few rounds have outsized impact for anchoring-prone jurors
        if round <= 3 {
            mult += self.anchoring * 0.6;
        }

        // Recency: later rounds (closing, deliberation) boost recency-biased jurors
        if round > 18 {
            mult += self.recency * 0.5;
        }

        // Sympathy: emotional arguments hit MUCH harder for sympathetic jurors
        // but barely affect analytical ones
        if emotional_weight > 0.3 {
            mult *= 1.0 + self.sympathy * emotional_weight * 0.8;
        } else {
            // Low-emotion arguments are boosted for authority-biased jurors
            mult *= 1.0 + self.authority * 0.3;
        }

        // Confirmation bias: if argument aligns with existing lean, it's amplified
        // (this is applied in apply_argument_impact, not here)

        mult
    }
}

// ---------------------------------------------------------------------------
// Trial-specific court actions (extends world::ActionType)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CourtAction {
    // Judge actions
    OpenCourt,
    SustainObjection,
    OverruleObjection,
    InstructJury { instruction: String },
    CallOrder,
    AdmitEvidence { evidence_id: String },
    StrikeFromRecord,

    // Attorney actions
    PresentArgument { content: String, evidence_refs: Vec<String> },
    CallWitness { witness_id: String },
    CrossExamine { witness_id: String, question: String },
    Object { grounds: ObjectionGrounds },
    ClosingStatement { content: String },

    // Witness actions
    Testify { content: String },
    AnswerQuestion { content: String },
    Hesitate,

    // Jury actions (deliberation only)
    DeliberateArgue { content: String },
    DeliberateAgree { with_juror: u8 },
    DeliberateChallenge { target_juror: u8, content: String },
    CastVote { verdict: Verdict },
}

// ---------------------------------------------------------------------------
// Trial state (sits alongside SimulationState)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrialState {
    pub schedule: TrialSchedule,
    pub current_phase: TrialPhase,
    pub juror_states: HashMap<Uuid, JurorState>,
    pub objection_history: Vec<ObjectionRecord>,
    pub evidence: Vec<Evidence>,
    pub transcript: Vec<TranscriptEntry>,
    pub momentum: Vec<(u32, f32)>, // (round, momentum) -1.0=defense, 1.0=prosecution
    pub verdict: Option<VerdictResult>,
    pub witness_order: Vec<(Uuid, Party)>, // (witness_id, called_by)
    pub current_witness: Option<Uuid>,
}

impl TrialState {
    pub fn new(schedule: TrialSchedule) -> Self {
        Self {
            current_phase: TrialPhase::Opening,
            schedule,
            juror_states: HashMap::new(),
            objection_history: Vec::new(),
            evidence: Vec::new(),
            transcript: Vec::new(),
            momentum: Vec::new(),
            verdict: None,
            witness_order: Vec::new(),
            current_witness: None,
        }
    }

    /// Register a juror.
    pub fn add_juror(&mut self, agent_id: Uuid, seat: u8) {
        self.juror_states.insert(agent_id, JurorState::new(seat));
    }

    /// Get current jury split: (guilty_count, undecided_count, innocent_count).
    pub fn jury_split(&self) -> (usize, usize, usize) {
        let mut guilty = 0;
        let mut undecided = 0;
        let mut innocent = 0;
        for js in self.juror_states.values() {
            if js.conviction > 0.2 {
                guilty += 1;
            } else if js.conviction < -0.2 {
                innocent += 1;
            } else {
                undecided += 1;
            }
        }
        (guilty, undecided, innocent)
    }

    /// Calculate current momentum (-1.0 = defense dominating, 1.0 = prosecution dominating).
    pub fn current_momentum(&self) -> f32 {
        if self.juror_states.is_empty() {
            return 0.0;
        }
        let total: f32 = self.juror_states.values().map(|j| j.conviction).sum();
        total / self.juror_states.len() as f32
    }

    /// Record momentum for this round.
    pub fn record_momentum(&mut self, round: u32) {
        self.momentum.push((round, self.current_momentum()));
    }

    /// Count momentum shifts (times the majority flipped).
    pub fn momentum_shifts(&self) -> usize {
        self.momentum.windows(2)
            .filter(|w| (w[0].1 > 0.0) != (w[1].1 > 0.0))
            .count()
    }

    /// Add a transcript entry.
    pub fn add_transcript(&mut self, entry: TranscriptEntry) {
        self.transcript.push(entry);
    }

    /// Record an objection and its ruling.
    pub fn record_objection(&mut self, record: ObjectionRecord) {
        self.objection_history.push(record);
    }

    /// Get objection stats: (sustained, overruled).
    pub fn objection_stats(&self) -> (usize, usize) {
        let sustained = self.objection_history.iter().filter(|o| o.sustained).count();
        let overruled = self.objection_history.len() - sustained;
        (sustained, overruled)
    }

    /// Perform final vote and set verdict.
    pub fn finalize_verdict(&mut self) -> VerdictResult {
        let mut guilty = 0;
        let mut not_guilty = 0;

        for js in self.juror_states.values_mut() {
            match js.cast_vote() {
                Verdict::Guilty => guilty += 1,
                Verdict::NotGuilty => not_guilty += 1,
            }
        }

        let result = VerdictResult {
            verdict: if guilty > not_guilty { Verdict::Guilty } else { Verdict::NotGuilty },
            guilty_votes: guilty,
            not_guilty_votes: not_guilty,
            unanimous: guilty == 0 || not_guilty == 0,
        };

        self.verdict = Some(result.clone());
        result
    }

    /// Get the average conviction across all jurors (for peer pressure).
    pub fn avg_conviction(&self) -> f32 {
        if self.juror_states.is_empty() {
            return 0.0;
        }
        let total: f32 = self.juror_states.values().map(|j| j.conviction).sum();
        total / self.juror_states.len() as f32
    }
}

// ---------------------------------------------------------------------------
// Supporting types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectionRecord {
    pub round: u32,
    pub objector: Uuid,
    pub objector_role: Party,
    pub grounds: ObjectionGrounds,
    pub sustained: bool,
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub id: String,
    pub description: String,
    pub presented_by: Party,
    pub presented_at_round: u32,
    pub admitted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptEntry {
    pub round: u32,
    pub speaker_id: Uuid,
    pub speaker_name: String,
    pub speaker_role: CourtRole,
    pub content: String,
    /// Impact on jury: vec of (juror_seat, conviction_delta)
    #[serde(default)]
    pub jury_impact: Vec<(u8, f32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerdictResult {
    pub verdict: Verdict,
    pub guilty_votes: usize,
    pub not_guilty_votes: usize,
    pub unanimous: bool,
}

// ---------------------------------------------------------------------------
// Trial-specific WebSocket events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum TrialWsEvent {
    #[serde(rename = "phase_change")]
    PhaseChange {
        phase: String,
        round: u32,
    },
    #[serde(rename = "argument")]
    Argument {
        speaker_id: String,
        speaker_name: String,
        speaker_role: String,
        content: String,
        jury_impact: Vec<JuryImpactEntry>,
    },
    #[serde(rename = "objection")]
    Objection {
        by: String,
        by_name: String,
        grounds: String,
        ruling: String,
    },
    #[serde(rename = "jury_shift")]
    JuryShift {
        seat: u8,
        old_conviction: f32,
        new_conviction: f32,
        confidence: f32,
        cause: String,
    },
    #[serde(rename = "witness_called")]
    WitnessCalled {
        name: String,
        witness_type: String,
        called_by: String,
    },
    #[serde(rename = "testimony")]
    Testimony {
        witness_id: String,
        witness_name: String,
        content: String,
        emotional_impact: f32,
    },
    #[serde(rename = "deliberation")]
    Deliberation {
        speaker_seat: u8,
        speaker_name: String,
        content: String,
    },
    #[serde(rename = "vote_cast")]
    VoteCast {
        seat: u8,
        name: String,
        verdict: String,
    },
    #[serde(rename = "verdict")]
    Verdict {
        result: String,
        guilty: usize,
        not_guilty: usize,
        unanimous: bool,
    },
    #[serde(rename = "trial_status")]
    TrialStatus {
        phase: String,
        round: u32,
        jury_split: (usize, usize, usize),
        momentum: f32,
        objections_sustained: usize,
        objections_overruled: usize,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct JuryImpactEntry {
    pub seat: u8,
    pub delta: f32,
    pub new_conviction: f32,
}

// ---------------------------------------------------------------------------
// Court-specific prompt builders
// ---------------------------------------------------------------------------

pub fn build_judge_system_prompt(
    judge_name: &str,
    case_summary: &str,
    phase: TrialPhase,
    recent_transcript: &str,
    pending_objections: &[String],
) -> String {
    let phase_instruction = match phase {
        TrialPhase::Opening => "Opening statements phase. First ask the Prosecution to deliver their opening statement, then the Defense. Keep it structured.",
        TrialPhase::ProsecutionCase => "Prosecution is presenting its case. Direct them to call witnesses, present evidence. Rule on any objections promptly. After witness testimony, allow cross-examination by the Defense.",
        TrialPhase::DefenseCase => "Defense is presenting its case. Direct them to call witnesses and present evidence. Rule on objections. After witness testimony, allow cross-examination by the Prosecution.",
        TrialPhase::Rebuttal => "Rebuttal phase. Allow the Prosecution to address specific points raised by the Defense. Keep it focused — no new evidence, only rebuttal of defense claims.",
        TrialPhase::Closing => "Closing arguments. Each side gives their final argument. Remind the jury about burden of proof and reasonable doubt before they deliberate.",
        TrialPhase::Deliberation => "Jury is deliberating. No further court proceedings.",
    };

    let objection_section = if pending_objections.is_empty() {
        String::new()
    } else {
        format!("\nPENDING OBJECTIONS TO RULE ON:\n{}\nYou MUST rule on each objection: sustain or overrule with brief reasoning.\n",
            pending_objections.join("\n"))
    };

    format!(
        r#"You are Judge {judge_name}, presiding over this criminal fraud trial.

CASE: {case_summary}

CURRENT PHASE: {phase}
PHASE INSTRUCTIONS: {phase_instruction}
{objection_section}
RECENT PROCEEDINGS:
{recent_transcript}

YOUR ROLE:
- Direct proceedings — tell attorneys what to do next
- Rule on ALL objections immediately (sustain or overrule with one-line reasoning)
- Instruct the jury when needed
- You are IMPARTIAL — never favor either side
- If attorneys are being repetitive, tell them to move on
- If a phase is ending, announce the transition

STYLE: Formal, authoritative, concise. 2-4 sentences max. Address attorneys as "Counsel".
NEVER prefix your speech with your own name.

RESPOND IN JSON:
{{
  "actions": [
    {{"action_type": "ruling", "content": "your statement to the court"}}
  ]
}}"#,
    )
}

pub fn build_attorney_system_prompt(
    attorney_name: &str,
    party: Party,
    case_summary: &str,
    phase: TrialPhase,
    recent_transcript: &str,
    arguments_already_made: &[String],
    jury_leaning: &str,
) -> String {
    let role = match party {
        Party::Prosecution => "PROSECUTOR",
        Party::Defense => "DEFENSE ATTORNEY",
    };
    let goal = match party {
        Party::Prosecution => "PROVE GUILT beyond reasonable doubt. Build a compelling narrative with evidence.",
        Party::Defense => "CREATE REASONABLE DOUBT. Challenge evidence, protect your client.",
    };

    let phase_action = match phase {
        TrialPhase::Opening => "Deliver your opening statement. Outline your case theory. Be specific about what evidence you will present.",
        TrialPhase::ProsecutionCase => match party {
            Party::Prosecution => "Present evidence, call witnesses, build your case. Ask specific questions to witnesses on the stand.",
            Party::Defense => "Cross-examine prosecution witnesses. Challenge their credibility and the evidence. Ask pointed questions.",
        },
        TrialPhase::DefenseCase => match party {
            Party::Defense => "Present your defense. Call your own witnesses. Present alternative explanations for the evidence.",
            Party::Prosecution => "Cross-examine defense witnesses. Challenge their credibility. Highlight inconsistencies.",
        },
        TrialPhase::Rebuttal => match party {
            Party::Prosecution => "Address specific claims made by the defense. Focus on rebutting their key arguments.",
            Party::Defense => "Counter the prosecution's rebuttal points. Reinforce reasonable doubt.",
        },
        TrialPhase::Closing => "Deliver your closing argument. Summarize the evidence. Make your strongest case to the jury. Reference SPECIFIC testimony and evidence from the trial.",
        TrialPhase::Deliberation => "Trial is over. No further arguments.",
    };

    let already_said = if arguments_already_made.is_empty() {
        String::new()
    } else {
        format!("\nARGUMENTS YOU ALREADY MADE (DO NOT REPEAT THESE — make NEW points):\n{}",
            arguments_already_made.iter().map(|a| format!("- {}", a)).collect::<Vec<_>>().join("\n"))
    };

    format!(
        r#"You are {attorney_name}, the {role} in this criminal fraud trial.

CASE: {case_summary}

CURRENT PHASE: {phase}
YOUR ACTION: {phase_action}
JURY STATUS: {jury_leaning}
{already_said}

RECENT PROCEEDINGS:
{recent_transcript}

YOUR GOAL: {goal}

RULES:
- You are {attorney_name}. You may use your name ONLY in your first opening statement.
- NEVER prefix your speech with your own name (e.g. don't start with "{attorney_name}:").
- Make a NEW argument each round — never repeat points already made.
- Be specific: reference evidence by name (Slack messages, audit, testimony).
- Keep it to 3-5 sentences. Real lawyers are concise in court.
- When examining a witness, ask ONE specific question, not a speech.

OBJECTIONS (Ace Attorney style):
- About 1 in 4 rounds, if the opposing counsel says something objectionable, you should
  dramatically object! Start your response with "OBJECTION!" followed by the legal grounds.
- Valid grounds: hearsay, speculation, leading the witness, relevance, asked and answered,
  assumes facts not in evidence, badgering the witness.
- Make it dramatic and pointed — channel Phoenix Wright. Be sharp, confident, theatrical.
- Example: "OBJECTION! Counsel is asking the witness to speculate about my client's intentions.
  There is no foundation for this line of questioning."
- Don't object every round — pick your moments for maximum impact. Only object when the
  opposing side actually says something objectionable in the recent proceedings.

RESPOND IN JSON:
{{
  "actions": [
    {{"action_type": "argument", "content": "what you say in court"}}
  ]
}}"#,
    )
}

pub fn build_witness_system_prompt(
    name: &str,
    witness_type: WitnessType,
    called_by: Party,
    backstory: &str,
    nervousness: f32,
    recent_transcript: &str,
    question_asked: &str,
) -> String {
    let type_desc = match witness_type {
        WitnessType::Expert => "EXPERT WITNESS — you provide professional/technical testimony",
        WitnessType::Eyewitness => "EYEWITNESS — you describe what you personally saw",
        WitnessType::Character => "CHARACTER WITNESS — you testify about the defendant's character",
    };
    let nervous_desc = if nervousness > 0.7 {
        "You are VERY NERVOUS. You stammer, pause, may contradict yourself."
    } else if nervousness > 0.4 {
        "You are somewhat nervous but trying to stay composed."
    } else {
        "You are calm and confident."
    };

    let called_by_str = match called_by {
        Party::Prosecution => "the Prosecution",
        Party::Defense => "the Defense",
    };

    format!(
        r#"You are {name}, testifying as a witness in a criminal fraud trial.

TYPE: {type_desc}
CALLED BY: {called_by_str}
YOUR BACKGROUND: {backstory}
DEMEANOR: {nervous_desc}

RECENT IN COURT:
{recent_transcript}

QUESTION ASKED TO YOU:
{question_asked}

RULES:
- Answer the question directly based on your character and what you know.
- Stay in character. Be specific — mention dates, names, details.
- 2-4 sentences. Real witnesses don't give speeches.
- You may show emotion if the topic is sensitive.
- If cross-examined aggressively, you can push back or get flustered.
- If you don't know something, say "I don't recall" or "I'm not sure."
- NEVER prefix your answer with your own name.

RESPOND IN JSON:
{{
  "actions": [
    {{"action_type": "testimony", "content": "your answer"}}
  ]
}}"#,
    )
}

pub fn build_jury_deliberation_prompt(
    juror_name: &str,
    seat: u8,
    conviction: f32,
    confidence: f32,
    key_moments: &[KeyMoment],
    peer_arguments: &[String],
) -> String {
    let stance = if conviction > 0.3 {
        "You currently lean GUILTY"
    } else if conviction < -0.3 {
        "You currently lean NOT GUILTY"
    } else {
        "You are UNDECIDED"
    };

    let moments_text: String = key_moments.iter().take(3)
        .map(|m| format!("- Round {}: \"{}\" (shifted your view by {:.2})", m.round, m.content_summary, m.conviction_delta))
        .collect::<Vec<_>>()
        .join("\n");

    let peer_text: String = peer_arguments.iter()
        .map(|a| format!("- {a}"))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"You are JUROR #{seat} ({juror_name}) during jury deliberation.

YOUR POSITION: {stance} (conviction: {conviction:.2}, confidence: {confidence:.2})

KEY MOMENTS THAT SHAPED YOUR VIEW:
{moments_text}

WHAT OTHER JURORS ARE SAYING:
{peer_text}

RULES:
- Argue your position based on evidence presented during the trial.
- You may be persuaded by other jurors' arguments — or push back.
- Reference specific testimony and evidence.
- Be human: express doubt, frustration, certainty.
- Keep it to 2-3 sentences.

RESPOND IN JSON:
{{
  "reasoning": "your thought process",
  "actions": [
    {{"action_type": "deliberate_argue|deliberate_agree|deliberate_challenge|cast_vote", "content": "...", "target_juror": N, "verdict": "guilty|not_guilty"}}
  ]
}}"#,
    )
}
