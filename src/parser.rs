use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;
use uuid::Uuid;

use crate::agent::{AgentProfile, BehaviorArchetype, Stance, Tier};
use crate::llm::LlmClient;

// ---------------------------------------------------------------------------
// Document parsing
// ---------------------------------------------------------------------------

/// Read a document and extract its text content.
/// Validates path is a regular file (not symlink to outside, not device, etc.).
pub fn parse_document(path: &Path, max_chars: usize) -> Result<String> {
    // Resolve to canonical path to prevent symlink-based path traversal
    let canonical = path
        .canonicalize()
        .with_context(|| format!("Cannot resolve path: {}", path.display()))?;

    // Ensure it's a regular file
    let metadata = std::fs::metadata(&canonical)
        .with_context(|| format!("Cannot read metadata: {}", canonical.display()))?;
    anyhow::ensure!(
        metadata.is_file(),
        "Not a regular file: {}",
        canonical.display()
    );

    let ext = canonical
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let text = match ext.as_str() {
        "md" | "markdown" | "txt" | "text" => {
            std::fs::read_to_string(&canonical)
                .with_context(|| format!("Failed to read {}", canonical.display()))?
        }
        "pdf" => parse_pdf(&canonical)?,
        other => {
            tracing::warn!("Unsupported document format: .{other}, trying as plain text");
            std::fs::read_to_string(&canonical)
                .with_context(|| format!("Failed to read {}", canonical.display()))?
        }
    };

    if text.len() > max_chars {
        Ok(text[..max_chars].to_string())
    } else {
        Ok(text)
    }
}

fn parse_pdf(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    pdf_extract::extract_text_from_mem(&bytes)
        .with_context(|| format!("Failed to extract text from PDF: {}", path.display()))
}

/// Split text into chunks at sentence boundaries.
pub fn chunk_text(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
    if text.len() <= chunk_size {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < text.len() {
        let end = (start + chunk_size).min(text.len());

        // Find a sentence boundary near the end
        let boundary = if end < text.len() {
            text[start..end]
                .rfind(". ")
                .or_else(|| text[start..end].rfind(".\n"))
                .or_else(|| text[start..end].rfind('\n'))
                .map(|i| start + i + 1)
                .unwrap_or(end)
        } else {
            end
        };

        chunks.push(text[start..boundary].to_string());
        start = if boundary > overlap {
            boundary - overlap
        } else {
            boundary
        };
    }

    chunks
}

// ---------------------------------------------------------------------------
// Stakeholder extraction via LLM
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ExtractedStakeholders {
    stakeholders: Vec<ExtractedStakeholder>,
}

#[derive(Debug, Deserialize)]
struct ExtractedStakeholder {
    name: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    bio: String,
    #[serde(default)]
    persona: String,
    #[serde(default)]
    stance: String,
    #[serde(default)]
    importance: String,
    #[serde(default)]
    interests: Vec<String>,
}

/// Extract stakeholders from seed documents and generate agent profiles.
pub async fn extract_and_generate_agents(
    llm: &LlmClient,
    documents: &[String],
    scenario: &str,
) -> Result<Vec<AgentProfile>> {
    // Step 1: Extract stakeholder PEOPLE from documents
    let mut all_stakeholders: Vec<ExtractedStakeholder> = Vec::new();

    let doc_text = documents.join("\n\n---\n\n");
    let doc_preview = if doc_text.len() > 6000 { &doc_text[..6000] } else { &doc_text };

    tracing::info!("Extracting stakeholder personas from scenario...");

    let system = r#"You generate realistic PEOPLE who would react to a scenario on social media.
CRITICAL RULES:
- Every stakeholder MUST be a PERSON (human being), NOT an organization, product, or service.
- If the scenario mentions companies/products (e.g. "Google", "Tesla", "iPhone"), create PEOPLE who work at, use, or are affected by them — NOT the company itself.
- Give each person a realistic full name, a Twitter-style username, and a clear emotional stake.
- Example: Instead of "Google Maps" → create "David Chen, delivery driver who relies on Google Maps daily"
- Example: Instead of "Tesla" → create "Sarah Lin, Tesla VP of Communications" or "Mike Torres, Uber driver worried about robotaxis"
- Diverse ages, genders, nationalities, professions.
Respond with ONLY valid JSON."#;

    let user = format!(
        r#"Scenario: {scenario}

Background context:
{doc_preview}

Generate 10-15 KEY STAKEHOLDER characters who would be actively posting on Twitter/X about this scenario.

Mix of:
- 3-4 high-profile figures (journalists, executives, politicians, experts) — importance: "high"
- 4-5 mid-profile (industry professionals, activists, content creators) — importance: "medium"
- 3-5 directly affected people (employees, customers, bystanders) — importance: "low"

Each person must have a CLEAR EMOTIONAL STAKE (angry, excited, scared, opportunistic, devastated, etc.)

JSON format:
{{"stakeholders": [
  {{"name": "Full Name", "username": "twitter_handle", "role": "their job/role", "bio": "1-line Twitter bio", "persona": "2-3 sentences: who they are, their emotional stake, how they personally relate to this scenario", "stance": "supportive|opposing|neutral|observer", "importance": "high|medium|low", "interests": ["topic1", "topic2"]}}
]}}"#
    );

    match llm.call_extraction(system, &user, 8192).await {
        Ok(raw) => {
            if let Ok(parsed) = serde_json::from_str::<ExtractedStakeholders>(&raw) {
                all_stakeholders.extend(parsed.stakeholders);
            } else if let Some(json_str) = extract_json_from_response(&raw) {
                if let Ok(parsed) = serde_json::from_str::<ExtractedStakeholders>(&json_str) {
                    all_stakeholders.extend(parsed.stakeholders);
                }
            }
        }
        Err(e) => tracing::warn!("Stakeholder extraction failed: {e}"),
    }

    // Deduplicate by username
    all_stakeholders.sort_by(|a, b| a.username.to_lowercase().cmp(&b.username.to_lowercase()));
    all_stakeholders.dedup_by(|a, b| a.username.to_lowercase() == b.username.to_lowercase());

    if all_stakeholders.is_empty() {
        tracing::warn!("No stakeholders extracted, generating default agents");
        return Ok(generate_default_agents(scenario));
    }

    tracing::info!("Extracted {} stakeholder personas", all_stakeholders.len());

    // Step 2: Convert stakeholders to agent profiles
    let mut agents: Vec<AgentProfile> = Vec::new();
    for s in &all_stakeholders {
        let tier = match s.importance.to_lowercase().as_str() {
            "high" => Tier::Tier1,
            "medium" => Tier::Tier2,
            _ => Tier::Tier3,
        };
        let stance = match s.stance.to_lowercase().as_str() {
            "supportive" => Stance::Supportive,
            "opposing" => Stance::Opposing,
            "observer" => Stance::Observer,
            _ => Stance::Neutral,
        };

        // Assign archetype based on tier and role
        let archetype = match tier {
            Tier::Tier1 => {
                let role = s.role.to_lowercase();
                if role.contains("journalist") || role.contains("reporter") || role.contains("editor") {
                    BehaviorArchetype::Journalist
                } else if role.contains("activist") || role.contains("advocate") || role.contains("organizer") {
                    BehaviorArchetype::Activist
                } else if role.contains("politician") || role.contains("senator") || role.contains("congress") {
                    BehaviorArchetype::Activist
                } else {
                    BehaviorArchetype::Analyst
                }
            }
            Tier::Tier2 => {
                let role = s.role.to_lowercase();
                if role.contains("journalist") || role.contains("reporter") || role.contains("creator") {
                    BehaviorArchetype::Journalist
                } else if role.contains("activist") || role.contains("advocate") || role.contains("union") {
                    BehaviorArchetype::Activist
                } else {
                    let roll: f32 = rand::random();
                    if roll < 0.35 { BehaviorArchetype::Analyst }
                    else if roll < 0.55 { BehaviorArchetype::Journalist }
                    else if roll < 0.75 { BehaviorArchetype::Activist }
                    else { BehaviorArchetype::Provocateur }
                }
            }
            Tier::Tier3 => assign_figurant_archetype(),
        };

        let username = if s.username.is_empty() {
            s.name.to_lowercase().replace(' ', "_")
                .chars().filter(|c| c.is_alphanumeric() || *c == '_')
                .take(20).collect::<String>()
        } else {
            s.username.replace('@', "")
                .chars().filter(|c| c.is_alphanumeric() || *c == '_')
                .take(20).collect::<String>()
        };

        agents.push(AgentProfile {
            id: Uuid::new_v4(),
            name: s.name.clone(),
            username,
            tier,
            bio: if s.bio.is_empty() { s.role.clone() } else { s.bio.clone() },
            persona: if s.persona.is_empty() {
                format!("{}: {}.", s.name, s.role)
            } else {
                s.persona.clone()
            },
            stance,
            sentiment_bias: match stance {
                Stance::Supportive => 0.5,
                Stance::Opposing => -0.5,
                Stance::Observer => 0.0,
                Stance::Neutral => 0.0,
            },
            influence_weight: match tier {
                Tier::Tier1 => 3.0,
                Tier::Tier2 => 1.5,
                Tier::Tier3 => 1.0,
            },
            activity_level: match tier {
                Tier::Tier1 => 0.9,
                Tier::Tier2 => 0.6,
                Tier::Tier3 => 0.3,
            },
            active_hours: (8..23).collect(),
            interests: s.interests.clone(),
            age: None,
            profession: Some(s.role.clone()),
            source_entity: None,
            archetype,
        });
    }

    // Step 3: Generate additional figurant agents (general public)
    let figurant_count = (agents.len() * 2).max(40).min(100);
    let existing_count = agents.len();
    if figurant_count > existing_count {
        let extra = figurant_count - existing_count;
        tracing::info!("Generating {extra} additional figurant agents");

        let system = r#"You generate realistic everyday social media users — regular people, NOT public figures.
Each person must feel REAL: specific job, specific emotional reaction to the scenario.
Give them realistic names and Twitter-style usernames.
Respond with ONLY valid JSON."#;

        let user = format!(
            r#"Generate {extra} diverse everyday social media users who would tweet about this scenario:
{scenario}

These are REGULAR PEOPLE (not experts or public figures). Mix of:
- People directly affected (lost access, disrupted workflow, financial impact)
- People with strong opinions (angry, amused, scared, opportunistic)
- Casual observers (confused, making jokes, sharing memes)
- People in different countries, ages (18-70), professions

Each person should have a SPECIFIC emotional reaction, not generic "interested observer".

JSON format:
{{"profiles": [{{"name": "Full Name", "username": "twitter_handle", "bio": "short Twitter bio", "persona": "Who they are and how this scenario affects them personally. Their emotional state.", "stance": "supportive|opposing|neutral|observer", "interests": ["topic1"]}}]}}"#
        );

        match llm.call_extraction(system, &user, 8192).await {
            Ok(raw) => {
                if let Ok(parsed) = parse_profile_response(&raw) {
                    for p in parsed {
                        let stance = match p.stance.to_lowercase().as_str() {
                            "supportive" => Stance::Supportive,
                            "opposing" => Stance::Opposing,
                            "observer" => Stance::Observer,
                            _ => Stance::Neutral,
                        };
                        let username = p.username.replace('@', "")
                            .chars().filter(|c| c.is_alphanumeric() || *c == '_')
                            .take(20).collect::<String>();
                        agents.push(AgentProfile {
                            id: Uuid::new_v4(),
                            name: p.name,
                            username,
                            tier: Tier::Tier3,
                            bio: p.bio,
                            persona: p.persona,
                            stance,
                            sentiment_bias: match stance {
                                Stance::Supportive => 0.3,
                                Stance::Opposing => -0.3,
                                _ => 0.0,
                            },
                            influence_weight: 1.0,
                            activity_level: 0.4,
                            active_hours: (8..23).collect(),
                            interests: p.interests,
                            age: None,
                            profession: None,
                            source_entity: None,
                            archetype: assign_figurant_archetype(),
                        });
                    }
                }
            }
            Err(e) => tracing::warn!("Figurant generation failed: {e}"),
        }
    }

    Ok(agents)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ProfileList {
    profiles: Vec<GeneratedProfile>,
}

#[derive(Deserialize)]
struct GeneratedProfile {
    name: String,
    username: String,
    bio: String,
    persona: String,
    #[serde(default = "default_neutral")]
    stance: String,
    #[serde(default)]
    interests: Vec<String>,
}

fn default_neutral() -> String {
    "neutral".into()
}

fn parse_profile_response(raw: &str) -> Result<Vec<GeneratedProfile>> {
    if let Ok(parsed) = serde_json::from_str::<ProfileList>(raw) {
        return Ok(parsed.profiles);
    }
    if let Some(json_str) = extract_json_from_response(raw) {
        if let Ok(parsed) = serde_json::from_str::<ProfileList>(&json_str) {
            return Ok(parsed.profiles);
        }
    }
    anyhow::bail!("Failed to parse profile response")
}

/// Assign a weighted random archetype for figurant (Tier3) agents.
fn assign_figurant_archetype() -> BehaviorArchetype {
    let roll: f32 = rand::random();
    if roll < 0.25 { BehaviorArchetype::Normie }
    else if roll < 0.45 { BehaviorArchetype::Lurker }
    else if roll < 0.60 { BehaviorArchetype::Cheerleader }
    else if roll < 0.75 { BehaviorArchetype::Activist }
    else if roll < 0.85 { BehaviorArchetype::Shitposter }
    else if roll < 0.95 { BehaviorArchetype::Provocateur }
    else { BehaviorArchetype::Journalist }
}

fn extract_json_from_response(s: &str) -> Option<String> {
    let s = s.trim();
    if let Some(start) = s.find("```") {
        let after = &s[start + 3..];
        let content_start = after.find('\n').map(|i| i + 1).unwrap_or(0);
        let content = &after[content_start..];
        if let Some(end) = content.find("```") {
            return Some(content[..end].trim().to_string());
        }
    }
    if let (Some(start), Some(end)) = (s.find('{'), s.rfind('}')) {
        if start < end {
            return Some(s[start..=end].to_string());
        }
    }
    None
}

fn generate_default_agents(scenario: &str) -> Vec<AgentProfile> {
    let defaults: Vec<(&str, &str, Tier, Stance, &str, &str, BehaviorArchetype)> = vec![
        ("Rachel Torres", "rachel_torres", Tier::Tier1, Stance::Neutral,
         "Breaking news reporter, 12 years at major networks",
         "Veteran journalist who lives for breaking stories. Always first to tweet, asks tough questions. Emotionally detached but sharp.",
         BehaviorArchetype::Journalist),
        ("Marcus Webb", "marcuswebb", Tier::Tier1, Stance::Neutral,
         "Senior tech analyst at Morgan Stanley",
         "Wall Street analyst who sees everything through market impact. Numbers-driven, unemotional, but respected for accuracy.",
         BehaviorArchetype::Analyst),
        ("Priya Sharma", "priya_speaks", Tier::Tier2, Stance::Opposing,
         "Workers rights advocate & labor organizer",
         "Passionate activist who frames everything as a fight between workers and corporations. Angry, urgent, calls to action.",
         BehaviorArchetype::Activist),
        ("James Liu", "jliu_tech", Tier::Tier2, Stance::Observer,
         "CTO at a mid-size startup, ex-FAANG",
         "Pragmatic tech leader who sees opportunities in chaos. Measured but opinionated. Thinks in systems.",
         BehaviorArchetype::Analyst),
        ("Derek Stone", "derekstone99", Tier::Tier2, Stance::Neutral,
         "VC partner, early-stage investments",
         "Investor who reacts to every news cycle with 'what does this mean for my portfolio'. Opportunistic, always networking.",
         BehaviorArchetype::Provocateur),
    ];

    let mut agents: Vec<AgentProfile> = defaults
        .iter()
        .map(|(name, username, tier, stance, bio, persona, archetype)| AgentProfile {
            id: Uuid::new_v4(),
            name: name.to_string(),
            username: username.to_string(),
            tier: *tier,
            bio: bio.to_string(),
            persona: format!("{persona} Scenario: {scenario}"),
            stance: *stance,
            sentiment_bias: match stance {
                Stance::Supportive => 0.5,
                Stance::Opposing => -0.5,
                _ => 0.0,
            },
            influence_weight: match tier {
                Tier::Tier1 => 3.0,
                Tier::Tier2 => 1.5,
                Tier::Tier3 => 1.0,
            },
            activity_level: match tier {
                Tier::Tier1 => 0.9,
                Tier::Tier2 => 0.6,
                Tier::Tier3 => 0.3,
            },
            active_hours: (8..23).collect(),
            interests: Vec::new(),
            age: None,
            profession: Some(bio.to_string()),
            source_entity: None,
            archetype: *archetype,
        })
        .collect();

    // Add 30 figurants with diverse personas
    let figurant_templates: Vec<(&str, &str, Stance, &str)> = vec![
        ("Alex K.", "alex_k_dev", Stance::Opposing, "Junior developer, frustrated and sarcastic"),
        ("Maria G.", "maria_g_mom", Stance::Neutral, "Working mom, confused and concerned"),
        ("Tyler R.", "tyler_memes", Stance::Observer, "College student, makes memes about everything"),
        ("Sandra P.", "sandra_phd", Stance::Neutral, "PhD researcher, overthinks every take"),
        ("Omar H.", "omar_hustle", Stance::Supportive, "Freelancer, always looking for the angle"),
        ("Jen W.", "jen_w_writes", Stance::Opposing, "Blogger, strong opinions, not afraid to say it"),
        ("Carlos M.", "carlos_m", Stance::Neutral, "Retired teacher, new to social media"),
        ("Aisha T.", "aisha_talks", Stance::Opposing, "Nursing student, empathetic and angry about injustice"),
        ("Dave B.", "dave_b_trades", Stance::Supportive, "Day trader, sees profit everywhere"),
        ("Kim L.", "kim_l_art", Stance::Observer, "Graphic designer, mostly lurks and likes"),
    ];

    for (i, (name, username, stance, persona)) in figurant_templates.iter().enumerate() {
        agents.push(AgentProfile {
            id: Uuid::new_v4(),
            name: name.to_string(),
            username: username.to_string(),
            tier: Tier::Tier3,
            bio: persona.to_string(),
            persona: format!("{persona}. Reacting to: {scenario}"),
            stance: *stance,
            sentiment_bias: match stance {
                Stance::Supportive => 0.3,
                Stance::Opposing => -0.3,
                _ => 0.0,
            },
            influence_weight: 1.0,
            activity_level: 0.4,
            active_hours: (8..23).collect(),
            interests: Vec::new(),
            age: None,
            profession: None,
            source_entity: None,
            archetype: assign_figurant_archetype(),
        });

        if i >= 9 { break; }
    }

    // Fill remaining with generic but varied users
    for i in 0..20 {
        let stance = match i % 4 {
            0 => Stance::Opposing,
            1 => Stance::Supportive,
            2 => Stance::Neutral,
            _ => Stance::Observer,
        };
        agents.push(AgentProfile {
            id: Uuid::new_v4(),
            name: format!("User{}", i + 11),
            username: format!("user_{}", 100 + i),
            tier: Tier::Tier3,
            bio: "Regular person on Twitter".into(),
            persona: format!("Average social media user reacting emotionally to: {scenario}"),
            stance,
            sentiment_bias: match stance {
                Stance::Supportive => 0.2,
                Stance::Opposing => -0.2,
                _ => 0.0,
            },
            influence_weight: 1.0,
            activity_level: 0.3,
            active_hours: (8..23).collect(),
            interests: Vec::new(),
            age: None,
            profession: None,
            source_entity: None,
            archetype: assign_figurant_archetype(),
        });
    }

    agents
}
