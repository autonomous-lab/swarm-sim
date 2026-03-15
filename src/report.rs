use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

use crate::agent::{AgentProfile, Tier};
use crate::engine::SimulationState;
use crate::llm::LlmClient;
use crate::world::RoundSummary;

/// Generate a final markdown report from the simulation results.
pub async fn generate_report(
    llm: &LlmClient,
    state: &SimulationState,
) -> Result<String> {
    // Build summary data for the LLM
    let total_rounds = state.world.round_summaries.len();
    let total_agents = state.agents.len();
    let total_posts = state.world.posts.len();
    let total_actions = state.total_actions;

    let tier_counts = count_by_tier(&state.agents);

    // Top posts by engagement
    let mut posts_sorted: Vec<_> = state.world.posts.values().collect();
    posts_sorted.sort_by(|a, b| {
        b.engagement_score()
            .partial_cmp(&a.engagement_score())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let top_posts: Vec<String> = posts_sorted
        .iter()
        .take(10)
        .map(|p| {
            format!(
                "- @{}: \"{}\" (likes:{}, replies:{}, reposts:{})",
                p.author_name,
                truncate(&p.content, 100),
                p.likes.len(),
                p.replies.len(),
                p.reposts.len(),
            )
        })
        .collect();

    // Agent activity summary
    // Activity is tracked via round summaries

    // Round trajectory
    let round_data: Vec<String> = state
        .world
        .round_summaries
        .iter()
        .map(|s| {
            format!(
                "R{}: {} active, {} posts, {} replies, {} likes",
                s.round, s.active_agents, s.new_posts, s.new_replies, s.new_likes
            )
        })
        .collect();

    // VIP agent summaries
    let vip_agents: Vec<String> = state
        .agents
        .values()
        .filter(|a| a.tier == Tier::Tier1)
        .map(|a| {
            let post_count = state
                .agent_states
                .get(&a.id)
                .map(|s| s.post_ids.len())
                .unwrap_or(0);
            let follower_count = state.world.social_graph.follower_count(&a.id);
            format!(
                "- @{} ({}): {} posts, {} followers, stance: {}",
                a.username, a.name, post_count, follower_count, a.stance
            )
        })
        .collect();

    let scenario = &state.config.simulation.scenario_prompt;

    let prompt = format!(
        r#"Generate a detailed simulation report in Markdown format.

SCENARIO: {scenario}

STATISTICS:
- Total rounds: {total_rounds}
- Total agents: {total_agents} (Tier1: {t1}, Tier2: {t2}, Tier3: {t3})
- Total posts: {total_posts}
- Total actions: {total_actions}

TOP 10 POSTS BY ENGAGEMENT:
{top_posts}

VIP AGENTS (TIER 1):
{vip_agents}

ROUND TRAJECTORY:
{round_trajectory}

Generate a comprehensive report with these sections:
1. Executive Summary (3-5 key findings)
2. Timeline of Key Events (major shifts and inflection points)
3. Agent Analysis (VIP behavior, most active, sentiment distribution)
4. Viral Content Analysis (top posts, cascade patterns)
5. Network Dynamics (opinion leaders, echo chambers)
6. Methodology Notes (tiers used, models, limitations)"#,
        t1 = tier_counts.0,
        t2 = tier_counts.1,
        t3 = tier_counts.2,
        top_posts = top_posts.join("\n"),
        vip_agents = vip_agents.join("\n"),
        round_trajectory = round_data.join("\n"),
    );

    let system = "You are an expert analyst generating a simulation report. \
        Write in clear, professional Markdown. Be specific and analytical. \
        Include quantitative observations where possible.";

    let report = llm.call_extraction(system, &prompt, 8192).await?;

    // Add metadata header
    let header = format!(
        "# Simulation Report\n\n\
        **Scenario:** {}\n\
        **Generated:** {}\n\
        **Rounds:** {} | **Agents:** {} | **Posts:** {} | **Actions:** {}\n\n---\n\n",
        scenario,
        chrono::Utc::now().format("%Y-%m-%d %H:%M UTC"),
        total_rounds,
        total_agents,
        total_posts,
        total_actions,
    );

    Ok(format!("{header}{report}"))
}

/// Save report to file.
pub async fn save_report(
    llm: &LlmClient,
    state: &SimulationState,
    output_dir: &Path,
    filename: &str,
) -> Result<String> {
    let report = generate_report(llm, state).await?;
    let path = output_dir.join(filename);
    std::fs::create_dir_all(output_dir)?;
    std::fs::write(&path, &report)?;
    tracing::info!("Report saved to {}", path.display());
    Ok(path.display().to_string())
}

fn count_by_tier(agents: &HashMap<uuid::Uuid, AgentProfile>) -> (usize, usize, usize) {
    let mut t1 = 0;
    let mut t2 = 0;
    let mut t3 = 0;
    for a in agents.values() {
        match a.tier {
            Tier::Tier1 => t1 += 1,
            Tier::Tier2 => t2 += 1,
            Tier::Tier3 => t3 += 1,
        }
    }
    (t1, t2, t3)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}
