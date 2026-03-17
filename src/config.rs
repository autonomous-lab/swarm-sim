use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Root config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimConfig {
    pub simulation: SimulationSettings,
    pub tiers: TierConfig,
    pub llm: LlmConfig,
    pub world: WorldConfig,
    pub parser: ParserConfig,
    pub output: OutputConfig,
    pub server: ServerConfig,
    pub god_eye: GodEyeConfig,
    #[serde(default)]
    pub synthesis: SynthesisConfig,
    #[serde(default)]
    pub webhooks: WebhookConfig,
}

// ---------------------------------------------------------------------------
// Sections
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationSettings {
    pub total_rounds: u32,
    pub minutes_per_round: u32,
    pub seed_documents: Vec<PathBuf>,
    pub scenario_prompt: String,
    #[serde(default)]
    pub random_seed: u64,
    #[serde(default)]
    pub challenge_question: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierConfig {
    pub tier1: TierSettings,
    pub tier2: TierSettings,
    pub tier3: TierSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierSettings {
    pub batch_size: usize,
    pub model: String,
    pub base_url: String,
    pub api_key: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: usize,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub input_price_per_mtok: f64,
    #[serde(default)]
    pub output_price_per_mtok: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub extraction_model: String,
    pub extraction_base_url: String,
    pub extraction_api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldConfig {
    #[serde(default = "default_feed_size")]
    pub feed_size: usize,
    #[serde(default = "default_trending_count")]
    pub trending_count: usize,
    #[serde(default = "default_weight")]
    pub recency_weight: f32,
    #[serde(default = "default_weight")]
    pub popularity_weight: f32,
    #[serde(default = "default_weight")]
    pub relevance_weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserConfig {
    #[serde(default = "default_max_chars")]
    pub max_chars_per_doc: usize,
    #[serde(default = "default_chunk_size")]
    pub chunk_size: usize,
    #[serde(default = "default_chunk_overlap")]
    pub chunk_overlap: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    #[serde(default = "default_output_dir")]
    pub output_dir: PathBuf,
    #[serde(default = "default_action_log")]
    pub action_log: String,
    #[serde(default = "default_report_file")]
    pub report_file: String,
    #[serde(default)]
    pub verbose: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GodEyeConfig {
    #[serde(default = "default_events_file")]
    pub events_file: PathBuf,
    #[serde(default = "default_debounce")]
    pub debounce_ms: u64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_synthesis_interval")]
    pub every_n_rounds: u32,
    #[serde(default = "default_synthesis_tokens")]
    pub max_tokens: u32,
}

impl Default for SynthesisConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            every_n_rounds: 3,
            max_tokens: 1024,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default)]
    pub enabled: bool,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            url: None,
            events: Vec::new(),
            enabled: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

fn default_temperature() -> f32 { 0.7 }
fn default_max_tokens() -> u32 { 4096 }
fn default_max_concurrency() -> usize { 10 }
fn default_max_retries() -> u32 { 2 }
fn default_timeout() -> u64 { 90 }
fn default_feed_size() -> usize { 15 }
fn default_trending_count() -> usize { 10 }
fn default_weight() -> f32 { 0.33 }
fn default_max_chars() -> usize { 50_000 }
fn default_chunk_size() -> usize { 4000 }
fn default_chunk_overlap() -> usize { 200 }
fn default_output_dir() -> PathBuf { PathBuf::from("./output") }
fn default_action_log() -> String { "actions.jsonl".into() }
fn default_report_file() -> String { "report.md".into() }
fn default_host() -> String { "0.0.0.0".into() }
fn default_port() -> u16 { 3000 }
fn default_events_file() -> PathBuf { PathBuf::from("./events.toml") }
fn default_debounce() -> u64 { 500 }
fn default_true() -> bool { true }
fn default_synthesis_interval() -> u32 { 3 }
fn default_synthesis_tokens() -> u32 { 1024 }

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

impl SimConfig {
    /// Load config from a TOML file, resolving `${ENV_VAR}` references.
    pub fn load(path: &std::path::Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config: {}", path.display()))?;
        let resolved = resolve_env_vars(&raw);
        let config: SimConfig =
            toml::from_str(&resolved).with_context(|| "Failed to parse config TOML")?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.simulation.total_rounds == 0 {
            anyhow::bail!("total_rounds must be > 0");
        }
        if self.simulation.minutes_per_round == 0 {
            anyhow::bail!("minutes_per_round must be > 0");
        }
        for (name, tier) in [
            ("tier1", &self.tiers.tier1),
            ("tier2", &self.tiers.tier2),
            ("tier3", &self.tiers.tier3),
        ] {
            if tier.batch_size == 0 {
                anyhow::bail!("{name}: batch_size must be > 0");
            }
            if tier.api_key.is_empty() || tier.api_key.starts_with("${") {
                anyhow::bail!("{name}: api_key not set (check env var)");
            }
        }
        if self.llm.extraction_api_key.is_empty()
            || self.llm.extraction_api_key.starts_with("${")
        {
            anyhow::bail!("llm.extraction_api_key not set (check env var)");
        }
        Ok(())
    }
}

/// Replace `${VAR}` with the corresponding env var value.
fn resolve_env_vars(input: &str) -> String {
    let mut result = input.to_string();
    while let Some(start) = result.find("${") {
        if let Some(end) = result[start..].find('}') {
            let var_name = &result[start + 2..start + end];
            let value = std::env::var(var_name).unwrap_or_default();
            result = format!("{}{}{}", &result[..start], value, &result[start + end + 1..]);
        } else {
            break;
        }
    }
    result
}
