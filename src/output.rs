use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};

use crate::agent::Tier;
use crate::world::{Action, ActionType, RoundSummary};

// ---------------------------------------------------------------------------
// JSONL logger
// ---------------------------------------------------------------------------

pub struct ActionLogger {
    writer: BufWriter<File>,
    path: PathBuf,
}

impl ActionLogger {
    pub fn new(output_dir: &Path, filename: &str) -> Result<Self> {
        fs::create_dir_all(output_dir)?;
        let path = output_dir.join(filename);
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;
        Ok(Self {
            writer: BufWriter::with_capacity(64 * 1024, file),
            path,
        })
    }

    pub fn log_action(&mut self, action: &Action) -> Result<()> {
        serde_json::to_writer(&mut self.writer, action)?;
        self.writer.write_all(b"\n")?;
        Ok(())
    }

    pub fn log_event(&mut self, event: &serde_json::Value) -> Result<()> {
        serde_json::to_writer(&mut self.writer, event)?;
        self.writer.write_all(b"\n")?;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        self.writer.flush()?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

// ---------------------------------------------------------------------------
// Terminal display
// ---------------------------------------------------------------------------

pub fn create_progress_bar(total_rounds: u32) -> ProgressBar {
    let pb = ProgressBar::new(total_rounds as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.cyan} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} rounds ({eta} remaining)")
            .unwrap()
            .progress_chars("##-"),
    );
    pb
}

pub fn print_action(action: &Action, verbose: bool) {
    if !verbose {
        return;
    }

    let tier_color = match action.agent_tier {
        Tier::Tier1 => "VIP".bright_yellow().bold(),
        Tier::Tier2 => "STD".bright_blue(),
        Tier::Tier3 => "FIG".bright_black(),
    };

    let action_str = match &action.action_type {
        ActionType::CreatePost => {
            let content = action.content.as_deref().unwrap_or("");
            let preview = if content.len() > 80 {
                format!("{}...", &content[..77])
            } else {
                content.to_string()
            };
            format!("{} \"{}\"", "POST".green().bold(), preview)
        }
        ActionType::Reply => {
            let content = action.content.as_deref().unwrap_or("");
            let preview = if content.len() > 60 {
                format!("{}...", &content[..57])
            } else {
                content.to_string()
            };
            let target = action
                .target_post_id
                .map(|id| id.to_string()[..8].to_string())
                .unwrap_or_default();
            format!("{} to {} \"{}\"", "REPLY".cyan(), target, preview)
        }
        ActionType::Like => {
            let target = action
                .target_post_id
                .map(|id| id.to_string()[..8].to_string())
                .unwrap_or_default();
            format!("{} {}", "LIKE".red(), target)
        }
        ActionType::Repost => {
            let target = action
                .target_post_id
                .map(|id| id.to_string()[..8].to_string())
                .unwrap_or_default();
            format!("{} {}", "REPOST".magenta(), target)
        }
        ActionType::Follow => {
            format!("{}", "FOLLOW".blue())
        }
        ActionType::Unfollow => {
            format!("{}", "UNFOLLOW".bright_black())
        }
        ActionType::DoNothing => {
            return; // Don't print do_nothing
        }
        ActionType::PinMemory => {
            let content = action.content.as_deref().unwrap_or("");
            format!("{} \"{}\"", "PIN".yellow(), content)
        }
    };

    println!(
        "  [{}] @{:<20} {}",
        tier_color, action.agent_name, action_str
    );
}

pub fn print_round_summary(summary: &RoundSummary) {
    println!(
        "\n{} R{:>3} | {} agents | {} posts {} replies {} likes {} reposts {} follows",
        ">>>".bright_cyan().bold(),
        summary.round,
        summary.active_agents.to_string().bright_white(),
        summary.new_posts.to_string().green(),
        summary.new_replies.to_string().cyan(),
        summary.new_likes.to_string().red(),
        summary.new_reposts.to_string().magenta(),
        summary.new_follows.to_string().blue(),
    );
    if summary.events_injected > 0 {
        println!(
            "  {} {} event(s) injected",
            "GOD'S EYE".bright_yellow().bold(),
            summary.events_injected
        );
    }
}

pub fn print_banner() {
    println!(
        "{}",
        r#"
  ____                                   ____  _
 / ___|_      ____ _ _ __ _ __ ___      / ___|(_)_ __ ___
 \___ \ \ /\ / / _` | '__| '_ ` _ \ ___\___ \| | '_ ` _ \
  ___) \ V  V / (_| | |  | | | | | |___|__) | | | | | | |
 |____/ \_/\_/ \__,_|_|  |_| |_| |_|  |____/|_|_| |_| |_|
"#
        .bright_cyan()
    );
    println!(
        "  {}",
        "Multi-agent social simulation with tiered LLM batching"
            .bright_black()
    );
    println!();
}
