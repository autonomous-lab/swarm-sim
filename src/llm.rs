use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agent::{AgentProfile, AgentState, Tier};
use crate::config::{TierConfig, TierSettings};
use crate::world::{Action, ActionType, Post};

// ---------------------------------------------------------------------------
// LLM client
// ---------------------------------------------------------------------------

pub struct LlmClient {
    clients: HashMap<Tier, (reqwest::Client, TierSettings)>,
    extraction_client: reqwest::Client,
    extraction_model: String,
    extraction_base_url: String,
    extraction_api_key: String,
}

impl LlmClient {
    pub fn new(
        tiers: &TierConfig,
        extraction_model: String,
        extraction_base_url: String,
        extraction_api_key: String,
    ) -> Self {
        let build = |s: &TierSettings| {
            reqwest::Client::builder()
                .pool_max_idle_per_host(s.max_concurrency)
                .pool_idle_timeout(Duration::from_secs(90))
                .timeout(Duration::from_secs(s.timeout_secs))
                .build()
                .expect("Failed to build HTTP client")
        };

        let mut clients = HashMap::new();
        clients.insert(Tier::Tier1, (build(&tiers.tier1), tiers.tier1.clone()));
        clients.insert(Tier::Tier2, (build(&tiers.tier2), tiers.tier2.clone()));
        clients.insert(Tier::Tier3, (build(&tiers.tier3), tiers.tier3.clone()));

        Self {
            clients,
            extraction_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("Failed to build extraction client"),
            extraction_model,
            extraction_base_url,
            extraction_api_key,
        }
    }

    pub fn settings_for(&self, tier: Tier) -> &TierSettings {
        &self.clients[&tier].1
    }

    /// Call the LLM for a simulation batch.
    pub async fn call_tier(
        &self,
        tier: Tier,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<String> {
        let (client, settings) = &self.clients[&tier];
        call_with_retry(
            client,
            &settings.base_url,
            &settings.api_key,
            &settings.model,
            settings.temperature,
            settings.max_tokens,
            system_prompt,
            user_prompt,
            settings.max_retries,
        )
        .await
    }

    /// Call the extraction/report LLM.
    pub async fn call_extraction(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        max_tokens: u32,
    ) -> Result<String> {
        call_with_retry(
            &self.extraction_client,
            &self.extraction_base_url,
            &self.extraction_api_key,
            &self.extraction_model,
            0.3,
            max_tokens,
            system_prompt,
            user_prompt,
            3,
        )
        .await
    }
}

// ---------------------------------------------------------------------------
// HTTP call with retry
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Deserialize)]
struct ChatMessageResponse {
    content: String,
}

/// Cap exponential backoff to avoid overflow (max ~2 minutes).
fn backoff_delay(base_ms: u64, attempt: u32) -> Duration {
    let capped_attempt = attempt.min(7); // max 2^7 = 128x multiplier
    Duration::from_millis(base_ms.saturating_mul(1u64 << capped_attempt))
}

/// Redact API keys and tokens from error text to prevent leaks in logs.
fn redact_sensitive(text: &str) -> String {
    // Redact Bearer tokens
    let re_bearer = regex_lite::Regex::new(r"Bearer\s+\S+").unwrap();
    let result = re_bearer.replace_all(text, "Bearer [REDACTED]");
    // Redact common key patterns (sk-..., key-..., etc.)
    let re_key = regex_lite::Regex::new(r"\b(sk-|key-|api[_-]?key[=:]\s*)\S+").unwrap();
    re_key.replace_all(&result, "[REDACTED_KEY]").to_string()
}

/// Max allowed LLM response size (1MB) to prevent memory exhaustion.
const MAX_RESPONSE_SIZE: usize = 1_048_576;

async fn call_with_retry(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    model: &str,
    temperature: f32,
    max_tokens: u32,
    system_prompt: &str,
    user_prompt: &str,
    max_retries: u32,
) -> Result<String> {
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let body = ChatRequest {
        model: model.to_string(),
        messages: vec![
            ChatMessage {
                role: "system".into(),
                content: system_prompt.into(),
            },
            ChatMessage {
                role: "user".into(),
                content: user_prompt.into(),
            },
        ],
        temperature,
        max_tokens,
    };

    let mut last_err = None;

    for attempt in 0..=max_retries {
        match client
            .post(&url)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    let text = response.text().await?;
                    if text.len() > MAX_RESPONSE_SIZE {
                        anyhow::bail!(
                            "LLM response too large ({} bytes, max {})",
                            text.len(),
                            MAX_RESPONSE_SIZE
                        );
                    }
                    let parsed: ChatResponse = serde_json::from_str(&text)
                        .with_context(|| "Failed to parse LLM response JSON")?;
                    if let Some(choice) = parsed.choices.first() {
                        let content = strip_think_tags(&choice.message.content);
                        return Ok(content);
                    }
                    anyhow::bail!("LLM returned empty choices");
                }
                if status.as_u16() == 429 {
                    let delay = backoff_delay(1000, attempt);
                    tracing::warn!("Rate limited (429), backing off {delay:?}");
                    tokio::time::sleep(delay).await;
                    continue;
                }
                if status.is_server_error() {
                    let delay = backoff_delay(500, attempt);
                    tracing::warn!("Server error {status}, retry in {delay:?}");
                    tokio::time::sleep(delay).await;
                    continue;
                }
                let body_text = response.text().await.unwrap_or_default();
                anyhow::bail!("LLM API error {status}: {}", redact_sensitive(&body_text));
            }
            Err(e) => {
                last_err = Some(e);
                let delay = backoff_delay(500, attempt);
                tokio::time::sleep(delay).await;
            }
        }
    }

    Err(last_err
        .map(|e| anyhow::anyhow!("LLM call failed after retries: {}", redact_sensitive(&e.to_string())))
        .unwrap_or_else(|| anyhow::anyhow!("LLM call failed")))
}

/// Remove `<think>...</think>` blocks (some models like DeepSeek/MiniMax emit these).
fn strip_think_tags(s: &str) -> String {
    let mut result = s.to_string();
    while let Some(start) = result.find("<think>") {
        if let Some(end) = result.find("</think>") {
            result = format!("{}{}", &result[..start], &result[end + 8..]);
        } else {
            break;
        }
    }
    result.trim().to_string()
}

// ---------------------------------------------------------------------------
// Response parsing (multi-layer fallback)
// ---------------------------------------------------------------------------

/// Parsed response from LLM for a single agent.
#[derive(Debug, Deserialize)]
pub struct SingleAgentResponse {
    #[serde(default)]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub actions: Vec<ParsedAction>,
    #[serde(default)]
    pub pin_memory: Option<String>,
}

/// Parsed response from LLM for a batch of agents.
#[derive(Debug, Deserialize)]
pub struct BatchAgentResponse {
    pub agent_actions: Vec<AgentActionEntry>,
}

#[derive(Debug, Deserialize)]
pub struct AgentActionEntry {
    pub agent_id: String,
    #[serde(default)]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub actions: Vec<ParsedAction>,
    #[serde(default)]
    pub pin_memory: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ParsedAction {
    pub action_type: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub target_post_id: Option<String>,
    #[serde(default)]
    pub target_agent_id: Option<String>,
}

/// Parse a single-agent LLM response with multi-layer fallback.
pub fn parse_single_response(raw: &str) -> Option<SingleAgentResponse> {
    // Layer 1: strict JSON
    if let Ok(r) = serde_json::from_str::<SingleAgentResponse>(raw) {
        return Some(r);
    }
    // Layer 2: extract from markdown code block
    if let Some(json_str) = extract_json_block(raw) {
        if let Ok(r) = serde_json::from_str::<SingleAgentResponse>(&json_str) {
            return Some(r);
        }
    }
    // Layer 3: fix truncated JSON
    let fixed = fix_truncated_json(raw);
    if let Ok(r) = serde_json::from_str::<SingleAgentResponse>(&fixed) {
        return Some(r);
    }
    None
}

/// Parse a batch LLM response with multi-layer fallback.
pub fn parse_batch_response(raw: &str) -> Option<BatchAgentResponse> {
    if let Ok(r) = serde_json::from_str::<BatchAgentResponse>(raw) {
        return Some(r);
    }
    if let Some(json_str) = extract_json_block(raw) {
        if let Ok(r) = serde_json::from_str::<BatchAgentResponse>(&json_str) {
            return Some(r);
        }
    }
    let fixed = fix_truncated_json(raw);
    if let Ok(r) = serde_json::from_str::<BatchAgentResponse>(&fixed) {
        return Some(r);
    }
    None
}

/// Extract JSON from markdown code blocks.
fn extract_json_block(s: &str) -> Option<String> {
    let s = s.trim();
    // ```json ... ``` or ``` ... ```
    if let Some(start) = s.find("```") {
        let after_backticks = &s[start + 3..];
        // Skip optional language identifier
        let content_start = after_backticks
            .find('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let content = &after_backticks[content_start..];
        if let Some(end) = content.find("```") {
            return Some(content[..end].trim().to_string());
        }
    }
    // Bare JSON object
    if let (Some(start), Some(end)) = (s.find('{'), s.rfind('}')) {
        if start < end {
            return Some(s[start..=end].to_string());
        }
    }
    None
}

/// Attempt to fix truncated JSON by closing open braces/brackets.
fn fix_truncated_json(s: &str) -> String {
    let s = s.trim();
    let start = s.find('{').unwrap_or(0);
    let mut result: String = s[start..].to_string();

    let mut open_braces = 0i32;
    let mut open_brackets = 0i32;
    let mut in_string = false;
    let mut escape = false;

    for ch in result.chars() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape = true,
            '"' => in_string = !in_string,
            '{' if !in_string => open_braces += 1,
            '}' if !in_string => open_braces -= 1,
            '[' if !in_string => open_brackets += 1,
            ']' if !in_string => open_brackets -= 1,
            _ => {}
        }
    }

    // Close any open strings
    if in_string {
        result.push('"');
    }
    // Close brackets then braces
    for _ in 0..open_brackets {
        result.push(']');
    }
    for _ in 0..open_braces {
        result.push('}');
    }

    result
}

// ---------------------------------------------------------------------------
// Prompt builders
// ---------------------------------------------------------------------------

/// Build the system prompt for a single (Tier 1) agent.
pub fn build_single_system_prompt(agent: &AgentProfile) -> String {
    format!(
        r#"You are simulating a social media user in a virtual community.

YOUR IDENTITY:
Name: {name}
Username: @{username}
Bio: {bio}
Persona: {persona}
Stance: {stance}
Sentiment: {sentiment:.1} (-1.0=very negative, 1.0=very positive)

INSTRUCTIONS:
Based on your feed, trending content, and recent events, decide what to do this round.
You may take 0-3 actions. Stay in character.

AVAILABLE ACTIONS:
- create_post: Write an original post. Include "content" field.
- reply: Reply to a post. Include "target_post_id" and "content" fields.
- like: Like a post. Include "target_post_id" field.
- repost: Repost someone's post. Include "target_post_id" field.
- follow: Follow a user. Include "target_agent_id" field.
- do_nothing: Skip this round.
- pin_memory: Mark something important to remember. Include "content" field.

Respond with ONLY valid JSON (no markdown, no explanation):
{{
  "reasoning": "Brief internal monologue",
  "actions": [
    {{"action_type": "...", "content": "...", "target_post_id": "...", "target_agent_id": "..."}}
  ],
  "pin_memory": "optional important observation"
}}"#,
        name = agent.name,
        username = agent.username,
        bio = agent.bio,
        persona = agent.persona,
        stance = agent.stance,
        sentiment = agent.sentiment_bias,
    )
}

/// Build the user prompt for a single agent (feed + trending + events + memory).
pub fn build_single_user_prompt(
    round: u32,
    total_rounds: u32,
    simulated_time: &str,
    memory_text: &str,
    feed_posts: &[PostSummary],
    trending_posts: &[PostSummary],
    events: &[String],
) -> String {
    let mut parts = Vec::new();

    parts.push(format!(
        "ROUND {round}/{total_rounds} | Time: {simulated_time}"
    ));

    if !memory_text.is_empty() {
        parts.push(format!("\nYOUR MEMORY:\n{memory_text}"));
    }

    if !feed_posts.is_empty() {
        parts.push("\nYOUR FEED:".into());
        for p in feed_posts {
            parts.push(format!(
                "  [{id}] @{author}: {content}\n    Likes:{likes} Replies:{replies} | {age}r ago",
                id = p.short_id,
                author = p.author,
                content = p.content_preview,
                likes = p.likes,
                replies = p.replies,
                age = p.rounds_ago,
            ));
        }
    }

    if !trending_posts.is_empty() {
        parts.push("\nTRENDING:".into());
        for (i, p) in trending_posts.iter().enumerate() {
            parts.push(format!(
                "  #{} [{id}] @{author}: {content} (engagement:{eng:.0})",
                i + 1,
                id = p.short_id,
                author = p.author,
                content = p.content_preview,
                eng = p.engagement,
            ));
        }
    }

    if !events.is_empty() {
        parts.push("\nBREAKING EVENTS THIS ROUND:".into());
        for e in events {
            parts.push(format!("  {e}"));
        }
    }

    parts.join("\n")
}

/// Build the system prompt for a batch of agents (Tier 2/3).
pub fn build_batch_system_prompt(agents: &[(AgentProfile, String)], persona_max_chars: usize) -> String {
    let mut agent_descs = String::new();
    for (agent, memory_short) in agents {
        agent_descs.push_str(&format!(
            "---\nID: {id}\nName: @{username} ({name})\nBio: {bio}\nPersona: {persona}\nStance: {stance}\nMemory: {memory}\n",
            id = &agent.id.to_string()[..8],
            username = agent.username,
            name = agent.name,
            bio = agent.bio,
            persona = agent.persona_truncated(persona_max_chars),
            stance = agent.stance,
            memory = memory_short,
        ));
    }

    format!(
        r#"You are simulating {n} social media users simultaneously.
Each user has their own personality. Generate actions for ALL of them.

USERS IN THIS BATCH:
{agent_descs}
AVAILABLE ACTIONS (per user, 0-2 actions each):
- create_post: Include "content"
- reply: Include "target_post_id" and "content"
- like: Include "target_post_id"
- repost: Include "target_post_id"
- follow: Include "target_agent_id"
- do_nothing: skip

Respond with ONLY valid JSON (no markdown):
{{
  "agent_actions": [
    {{
      "agent_id": "ID",
      "reasoning": "brief",
      "actions": [{{"action_type": "...", ...}}],
      "pin_memory": "optional"
    }}
  ]
}}"#,
        n = agents.len(),
    )
}

/// Build the user prompt for a batch.
pub fn build_batch_user_prompt(
    round: u32,
    total_rounds: u32,
    simulated_time: &str,
    feed_posts: &[PostSummary],
    trending_posts: &[PostSummary],
    prior_tier_actions: &[String],
    events: &[String],
) -> String {
    let mut parts = Vec::new();

    parts.push(format!(
        "ROUND {round}/{total_rounds} | Time: {simulated_time}"
    ));

    if !feed_posts.is_empty() {
        parts.push("\nSHARED FEED (top posts):".into());
        for p in feed_posts.iter().take(10) {
            parts.push(format!(
                "  [{id}] @{author}: {content} (L:{likes} R:{replies})",
                id = p.short_id,
                author = p.author,
                content = p.content_preview,
                likes = p.likes,
                replies = p.replies,
            ));
        }
    }

    if !prior_tier_actions.is_empty() {
        parts.push("\nRECENT ACTIVITY FROM KEY FIGURES:".into());
        for a in prior_tier_actions.iter().take(15) {
            parts.push(format!("  {a}"));
        }
    }

    if !trending_posts.is_empty() {
        parts.push("\nTRENDING:".into());
        for (i, p) in trending_posts.iter().take(5).enumerate() {
            parts.push(format!(
                "  #{} @{}: {} (eng:{:.0})",
                i + 1,
                p.author,
                p.content_preview,
                p.engagement,
            ));
        }
    }

    if !events.is_empty() {
        parts.push("\nBREAKING EVENTS:".into());
        for e in events {
            parts.push(format!("  {e}"));
        }
    }

    parts.join("\n")
}

// ---------------------------------------------------------------------------
// Post summary (for prompt building)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostSummary {
    pub short_id: String,
    pub author: String,
    pub content_preview: String,
    pub likes: usize,
    pub replies: usize,
    pub rounds_ago: u32,
    pub engagement: f64,
}

impl PostSummary {
    pub fn from_post(post: &Post, current_round: u32, max_content: usize) -> Self {
        let preview = if post.content.len() > max_content {
            format!("{}...", &post.content[..max_content.min(post.content.len())])
        } else {
            post.content.clone()
        };
        Self {
            short_id: post.short_id(),
            author: post.author_name.clone(),
            content_preview: preview,
            likes: post.likes.len(),
            replies: post.replies.len(),
            rounds_ago: current_round.saturating_sub(post.created_at_round),
            engagement: post.engagement_score(),
        }
    }
}
