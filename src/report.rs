use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

use crate::agent::{AgentProfile, Tier};
use crate::engine::SimulationState;
use crate::llm::LlmClient;

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

    // Top solutions (if challenge mode)
    let solutions_section = if !state.world.solution_ids.is_empty() {
        let mut solution_posts: Vec<_> = state
            .world
            .solution_ids
            .iter()
            .filter_map(|id| state.world.posts.get(id))
            .collect();
        solution_posts.sort_by(|a, b| {
            b.engagement_score()
                .partial_cmp(&a.engagement_score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let top: Vec<String> = solution_posts
            .iter()
            .take(10)
            .map(|p| {
                format!(
                    "- @{}: \"{}\" (likes:{}, replies:{}, reposts:{})",
                    p.author_name,
                    truncate(&p.content, 150),
                    p.likes.len(),
                    p.replies.len(),
                    p.reposts.len(),
                )
            })
            .collect();
        format!(
            "\nTOP SOLUTIONS ({} total):\n{}",
            state.world.solution_ids.len(),
            top.join("\n")
        )
    } else {
        String::new()
    };

    let challenge_section = state
        .config
        .simulation
        .challenge_question
        .as_deref()
        .map(|q| format!("\nCHALLENGE QUESTION: {q}"))
        .unwrap_or_default();

    // Compute metrics for the report
    let metrics = crate::metrics::compute_metrics(state);

    let metrics_section = format!(
        "\nKEY METRICS:\n\
         - Polarization index: {pol:.2} (0=consensus, 1=divided)\n\
         - Sentiment drift: {drift:.2}\n\
         - Stance switches: {switches}\n\
         - Viral posts: {viral}\n\
         - Cascade count: {cascades}\n\
         - Engagement Gini: {gini:.2} (0=equal, 1=concentrated)\n\
         - Top 10% engagement share: {top10:.0}%\n\
         - Echo chamber score: {echo:.2} (0=diverse, 1=echo)\n\
         - Cross-stance interactions: {cross}\n\
         - Contested posts: {contested}\n\
         - Average fatigue: {fatigue:.2}\n\
         - Average belief strength: {belief:.2}",
        pol = metrics.polarization.polarization_index,
        drift = metrics.polarization.average_sentiment_drift,
        switches = metrics.polarization.stance_switches,
        viral = metrics.virality.viral_post_count,
        cascades = metrics.virality.cascade_count,
        gini = metrics.influence.engagement_gini,
        top10 = metrics.influence.top_10_pct_share * 100.0,
        echo = metrics.community.echo_chamber_score,
        cross = metrics.community.cross_stance_interactions,
        contested = metrics.content.contested_count,
        fatigue = metrics.cognitive.avg_fatigue,
        belief = metrics.cognitive.avg_belief_strength,
    );

    let controversial = if !metrics.cognitive.controversial_topics.is_empty() {
        let topics: Vec<String> = metrics.cognitive.controversial_topics.iter()
            .take(5)
            .map(|(t, v)| format!("  - {}: variance {:.2}", t, v))
            .collect();
        format!("\nMOST CONTROVERSIAL TOPICS:\n{}", topics.join("\n"))
    } else {
        String::new()
    };

    let scenario = &state.config.simulation.scenario_prompt;

    let prompt = format!(
        r#"Generate a detailed simulation report in Markdown format.

SCENARIO: {scenario}{challenge_section}

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
{metrics_section}{controversial}
{solutions_section}

Generate a comprehensive report with these sections:
1. Executive Summary (3-5 key findings)
2. Timeline of Key Events (major shifts and inflection points)
3. Agent Analysis (VIP behavior, most active, sentiment distribution)
4. Viral Content Analysis (top posts, cascade patterns)
5. Network Dynamics (opinion leaders, echo chambers, cross-stance engagement)
6. Quantitative Analysis (use the KEY METRICS above: polarization index, Gini, echo chamber score, cascade analysis){solutions_report_section}
7. Methodology Notes (tiers used, models, limitations)"#,
        t1 = tier_counts.0,
        t2 = tier_counts.1,
        t3 = tier_counts.2,
        top_posts = top_posts.join("\n"),
        vip_agents = vip_agents.join("\n"),
        round_trajectory = round_data.join("\n"),
        metrics_section = metrics_section,
        controversial = controversial,
        solutions_section = solutions_section,
        solutions_report_section = if !state.world.solution_ids.is_empty() {
            "\n7. Top Proposed Solutions (ranked by community engagement, analysis of themes)"
        } else {
            ""
        },
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
