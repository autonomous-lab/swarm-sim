use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;
use uuid::Uuid;

use crate::agent::{AgentProfile, Stance, Tier};
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
// Entity extraction via LLM
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ExtractedEntities {
    entities: Vec<ExtractedEntity>,
}

#[derive(Debug, Deserialize)]
struct ExtractedEntity {
    name: String,
    #[serde(default)]
    entity_type: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    stance: String,
    #[serde(default)]
    importance: String,
}

/// Extract entities from seed documents and generate agent profiles.
pub async fn extract_and_generate_agents(
    llm: &LlmClient,
    documents: &[String],
    scenario: &str,
) -> Result<Vec<AgentProfile>> {
    // Step 1: Extract entities from all document chunks
    let mut all_entities: Vec<ExtractedEntity> = Vec::new();

    for (i, doc) in documents.iter().enumerate() {
        tracing::info!("Extracting entities from document chunk {}", i + 1);

        let system = "You are an entity extraction system. Extract all notable entities \
            (people, organizations, groups, media, public figures) from the text. \
            For each, provide name, type, description, stance (if identifiable), \
            and importance (high/medium/low). Respond with ONLY valid JSON.";

        let user = format!(
            "Extract entities from this text:\n\n{doc}\n\n\
            Context scenario: {scenario}\n\n\
            Respond with JSON:\n\
            {{\"entities\": [{{\"name\": \"...\", \"entity_type\": \"Person|Organization|Group|Media|Government\", \
            \"description\": \"...\", \"stance\": \"supportive|opposing|neutral|observer\", \
            \"importance\": \"high|medium|low\"}}]}}"
        );

        match llm.call_extraction(system, &user, 4096).await {
            Ok(raw) => {
                if let Ok(parsed) = serde_json::from_str::<ExtractedEntities>(&raw) {
                    all_entities.extend(parsed.entities);
                } else if let Some(json_str) = extract_json_from_response(&raw) {
                    if let Ok(parsed) = serde_json::from_str::<ExtractedEntities>(&json_str) {
                        all_entities.extend(parsed.entities);
                    }
                }
            }
            Err(e) => tracing::warn!("Entity extraction failed for chunk {i}: {e}"),
        }
    }

    // Deduplicate by name (case-insensitive)
    all_entities.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    all_entities.dedup_by(|a, b| a.name.to_lowercase() == b.name.to_lowercase());

    if all_entities.is_empty() {
        tracing::warn!("No entities extracted, generating default agents");
        return Ok(generate_default_agents(scenario));
    }

    tracing::info!("Extracted {} unique entities", all_entities.len());

    // Step 2: Generate agent profiles
    let mut agents: Vec<AgentProfile> = Vec::new();
    for entity in &all_entities {
        let tier = match entity.importance.to_lowercase().as_str() {
            "high" => Tier::Tier1,
            "medium" => Tier::Tier2,
            _ => Tier::Tier3,
        };
        let stance = match entity.stance.to_lowercase().as_str() {
            "supportive" => Stance::Supportive,
            "opposing" => Stance::Opposing,
            "observer" => Stance::Observer,
            _ => Stance::Neutral,
        };

        let username = entity
            .name
            .to_lowercase()
            .replace(' ', "_")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .take(20)
            .collect::<String>();

        agents.push(AgentProfile {
            id: Uuid::new_v4(),
            name: entity.name.clone(),
            username,
            tier,
            bio: entity.description.clone(),
            persona: format!(
                "{}: {}. Type: {}.",
                entity.name, entity.description, entity.entity_type
            ),
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
            interests: Vec::new(),
            age: None,
            profession: Some(entity.entity_type.clone()),
            source_entity: Some(entity.name.clone()),
        });
    }

    // Step 3: Generate additional figurant agents to fill out the simulation
    let figurant_count = (agents.len() * 2).max(20).min(100);
    let existing_count = agents.len();
    if figurant_count > existing_count {
        let extra = figurant_count - existing_count;
        tracing::info!("Generating {extra} additional figurant agents");

        let system = "You generate realistic social media user profiles. \
            Respond with ONLY valid JSON.";
        let user = format!(
            "Generate {extra} diverse social media user profiles for a simulation about: {scenario}\n\
            These are general public members (not key figures). Make them diverse in age, \
            profession, stance, and personality.\n\n\
            JSON format:\n\
            {{\"profiles\": [{{\"name\": \"...\", \"username\": \"...\", \"bio\": \"...\", \
            \"persona\": \"...\", \"stance\": \"supportive|opposing|neutral|observer\", \
            \"interests\": [\"...\"]}}]}}"
        );

        match llm.call_extraction(system, &user, 8192).await {
            Ok(raw) => {
                if let Ok(parsed) = parse_profile_response(&raw) {
                    for p in parsed {
                        agents.push(AgentProfile {
                            id: Uuid::new_v4(),
                            name: p.name,
                            username: p.username,
                            tier: Tier::Tier3,
                            bio: p.bio,
                            persona: p.persona,
                            stance: match p.stance.to_lowercase().as_str() {
                                "supportive" => Stance::Supportive,
                                "opposing" => Stance::Opposing,
                                "observer" => Stance::Observer,
                                _ => Stance::Neutral,
                            },
                            sentiment_bias: 0.0,
                            influence_weight: 1.0,
                            activity_level: 0.3,
                            active_hours: (8..23).collect(),
                            interests: p.interests,
                            age: None,
                            profession: None,
                            source_entity: None,
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
    let defaults = [
        ("News Anchor", "news_anchor", Tier::Tier1, Stance::Neutral, "Veteran journalist covering breaking news"),
        ("Industry Analyst", "analyst", Tier::Tier1, Stance::Neutral, "Tech industry analyst with 15 years experience"),
        ("Employee Rep", "employee_rep", Tier::Tier2, Stance::Opposing, "Employee advocacy leader"),
        ("Competitor CEO", "competitor_ceo", Tier::Tier2, Stance::Observer, "CEO of a competing company"),
        ("Investor", "investor_42", Tier::Tier2, Stance::Neutral, "Venture capitalist and angel investor"),
    ];

    let mut agents: Vec<AgentProfile> = defaults
        .iter()
        .map(|(name, username, tier, stance, bio)| AgentProfile {
            id: Uuid::new_v4(),
            name: name.to_string(),
            username: username.to_string(),
            tier: *tier,
            bio: bio.to_string(),
            persona: format!("{name}: {bio}. Scenario context: {scenario}"),
            stance: *stance,
            sentiment_bias: 0.0,
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
            profession: None,
            source_entity: None,
        })
        .collect();

    // Add 20 figurants
    for i in 0..20 {
        agents.push(AgentProfile {
            id: Uuid::new_v4(),
            name: format!("User {}", i + 1),
            username: format!("user_{}", i + 1),
            tier: Tier::Tier3,
            bio: "General public observer".into(),
            persona: format!("Ordinary social media user interested in the scenario: {scenario}"),
            stance: Stance::Neutral,
            sentiment_bias: 0.0,
            influence_weight: 1.0,
            activity_level: 0.3,
            active_hours: (8..23).collect(),
            interests: Vec::new(),
            age: None,
            profession: None,
            source_entity: None,
        });
    }

    agents
}
