use std::collections::HashMap;

use serde::Serialize;
use uuid::Uuid;

use crate::agent::Stance;
use crate::engine::SimulationState;

// ---------------------------------------------------------------------------
// Full metrics snapshot — computed on demand from SimulationState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct SimulationMetrics {
    pub polarization: PolarizationMetrics,
    pub virality: ViralityMetrics,
    pub influence: InfluenceMetrics,
    pub contagion: ContagionMetrics,
    pub community: CommunityMetrics,
    pub content: ContentMetrics,
    pub cognitive: CognitiveMetrics,
}

// ---------------------------------------------------------------------------
// Polarization: how divided is the population?
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct PolarizationMetrics {
    /// Esteban-Ray polarization index (0.0 = consensus, 1.0 = perfectly divided).
    pub polarization_index: f32,
    /// Distribution of sentiments across buckets.
    pub sentiment_distribution: Vec<SentimentBucket>,
    /// How much sentiments changed from initial to current.
    pub average_sentiment_drift: f32,
    /// Agents that switched stance (supportive <-> opposing).
    pub stance_switches: usize,
    /// Per-round polarization trend.
    pub polarization_trend: Vec<(u32, f32)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SentimentBucket {
    pub range: String,
    pub count: usize,
}

// ---------------------------------------------------------------------------
// Virality: what content spread and how?
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ViralityMetrics {
    /// Number of active cascades (posts with cascade_root set).
    pub cascade_count: usize,
    /// Largest cascade (root_id, size, max_depth).
    pub largest_cascade: Option<CascadeInfo>,
    /// Average cascade depth.
    pub avg_cascade_depth: f32,
    /// Posts that went viral (engagement velocity > threshold).
    pub viral_post_count: usize,
    /// Top viral posts by engagement velocity.
    pub top_viral: Vec<ViralPost>,
    /// Repost-to-original ratio (indicator of amplification).
    pub amplification_ratio: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct CascadeInfo {
    pub root_id: String,
    pub root_author: String,
    pub size: usize,
    pub max_depth: u32,
    pub root_content_preview: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ViralPost {
    pub post_id: String,
    pub author: String,
    pub content_preview: String,
    pub engagement: f64,
    pub velocity: f64,
}

// ---------------------------------------------------------------------------
// Influence: who drives the conversation?
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct InfluenceMetrics {
    /// Top influencers by combined follower + engagement score.
    pub top_influencers: Vec<InfluencerInfo>,
    /// Gini coefficient of engagement distribution (0 = equal, 1 = one person gets all).
    pub engagement_gini: f32,
    /// How concentrated is influence among top 10% of agents.
    pub top_10_pct_share: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct InfluencerInfo {
    pub agent_id: String,
    pub username: String,
    pub tier: String,
    pub follower_count: usize,
    pub total_engagement: f64,
    pub influence_score: f64,
}

// ---------------------------------------------------------------------------
// Contagion: how fast do ideas spread?
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ContagionMetrics {
    /// Average rounds for a post to reach peak engagement.
    pub avg_time_to_peak: f32,
    /// Fastest spreading post (rounds from creation to 50% of final engagement).
    pub fastest_spread: Option<SpreadInfo>,
    /// Topic persistence: how many rounds each topic stays active.
    pub topic_persistence: Vec<TopicPersistence>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpreadInfo {
    pub post_id: String,
    pub author: String,
    pub rounds_to_peak: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopicPersistence {
    pub topic: String,
    pub first_round: u32,
    pub last_round: u32,
    pub duration: u32,
    pub post_count: usize,
}

// ---------------------------------------------------------------------------
// Community: structure of the social graph
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct CommunityMetrics {
    /// Number of connected components.
    pub components: usize,
    /// Number of isolated agents (0 followers AND 0 following).
    pub isolated_agents: usize,
    /// Average clustering coefficient (how many of your friends know each other).
    pub avg_clustering: f32,
    /// Graph density (actual edges / possible edges).
    pub density: f32,
    /// Number of cross-stance interactions (opposing agents engaging).
    pub cross_stance_interactions: usize,
    /// Echo chamber score: how often agents only engage with same-stance peers (0 = diverse, 1 = echo chamber).
    pub echo_chamber_score: f32,
}

// ---------------------------------------------------------------------------
// Content: what kind of content is being produced?
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ContentMetrics {
    /// Total posts, replies, reposts, quotes.
    pub post_count: usize,
    pub reply_count: usize,
    pub repost_count: usize,
    pub quote_count: usize,
    /// Contested posts (with opposing replies).
    pub contested_count: usize,
    /// Average post length.
    pub avg_post_length: f32,
    /// Top hashtags.
    pub top_hashtags: Vec<(String, usize)>,
}

// ---------------------------------------------------------------------------
// Cognitive: fatigue and engagement patterns
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct CognitiveMetrics {
    /// Average fatigue across all agents.
    pub avg_fatigue: f32,
    /// Agents currently exhausted (fatigue > 0.7).
    pub exhausted_count: usize,
    /// Average belief strength (how opinionated are agents?).
    pub avg_belief_strength: f32,
    /// Most controversial topics (highest variance in beliefs).
    pub controversial_topics: Vec<(String, f32)>,
}

// ---------------------------------------------------------------------------
// Compute all metrics from state
// ---------------------------------------------------------------------------

pub fn compute_metrics(state: &SimulationState) -> SimulationMetrics {
    SimulationMetrics {
        polarization: compute_polarization(state),
        virality: compute_virality(state),
        influence: compute_influence(state),
        contagion: compute_contagion(state),
        community: compute_community(state),
        content: compute_content(state),
        cognitive: compute_cognitive(state),
    }
}

fn compute_polarization(state: &SimulationState) -> PolarizationMetrics {
    let sentiments: Vec<f32> = state.agent_states.values()
        .map(|st| st.current_sentiment)
        .collect();

    // Sentiment distribution in 5 buckets
    let buckets = vec![
        ("very_negative (-1.0 to -0.6)", -1.0f32, -0.6),
        ("negative (-0.6 to -0.2)", -0.6, -0.2),
        ("neutral (-0.2 to 0.2)", -0.2, 0.2),
        ("positive (0.2 to 0.6)", 0.2, 0.6),
        ("very_positive (0.6 to 1.0)", 0.6, 1.0),
    ];
    let sentiment_distribution: Vec<SentimentBucket> = buckets.iter()
        .map(|(name, low, high)| SentimentBucket {
            range: name.to_string(),
            count: sentiments.iter().filter(|&&s| s >= *low && s < *high).count(),
        })
        .collect();

    // Polarization index: variance of sentiment (simplified)
    let n = sentiments.len() as f32;
    if n == 0.0 {
        return PolarizationMetrics {
            polarization_index: 0.0,
            sentiment_distribution,
            average_sentiment_drift: 0.0,
            stance_switches: 0,
            polarization_trend: Vec::new(),
        };
    }
    let mean: f32 = sentiments.iter().sum::<f32>() / n;
    let variance: f32 = sentiments.iter().map(|s| (s - mean).powi(2)).sum::<f32>() / n;
    // Normalize to 0-1 range (max variance when split between -1 and 1 is 1.0)
    let polarization_index = variance.min(1.0);

    // Sentiment drift
    let drift: f32 = state.agent_states.values()
        .filter_map(|st| {
            let initial = st.sentiment_history.first().map(|(_, s)| *s)?;
            Some((st.current_sentiment - initial).abs())
        })
        .sum::<f32>() / n.max(1.0);

    // Stance switches
    let stance_switches = state.agents.iter()
        .filter(|(id, profile)| {
            let initial_stance = profile.stance;
            let current_sentiment = state.agent_states.get(id)
                .map(|st| st.current_sentiment)
                .unwrap_or(0.0);
            let current_stance = Stance::from_sentiment(current_sentiment);
            matches!(
                (initial_stance, current_stance),
                (Stance::Supportive, Stance::Opposing) | (Stance::Opposing, Stance::Supportive)
            )
        })
        .count();

    // Per-round polarization trend
    let max_round = state.world.current_round;
    let polarization_trend: Vec<(u32, f32)> = (1..=max_round)
        .step_by(1.max(max_round as usize / 20))
        .map(|round| {
            let sents: Vec<f32> = state.agent_states.values()
                .filter_map(|st| {
                    st.sentiment_history.iter()
                        .filter(|(r, _)| *r <= round)
                        .last()
                        .map(|(_, s)| *s)
                })
                .collect();
            let n = sents.len() as f32;
            if n == 0.0 { return (round, 0.0); }
            let mean = sents.iter().sum::<f32>() / n;
            let var = sents.iter().map(|s| (s - mean).powi(2)).sum::<f32>() / n;
            (round, var.min(1.0))
        })
        .collect();

    PolarizationMetrics {
        polarization_index,
        sentiment_distribution,
        average_sentiment_drift: drift,
        stance_switches,
        polarization_trend,
    }
}

fn compute_virality(state: &SimulationState) -> ViralityMetrics {
    let current_round = state.world.current_round;
    let cascade_stats = state.world.cascade_stats();

    let largest_cascade = cascade_stats.first().and_then(|(root_id, size, depth)| {
        state.world.posts.get(root_id).map(|p| CascadeInfo {
            root_id: root_id.to_string(),
            root_author: p.author_name.clone(),
            size: *size,
            max_depth: *depth,
            root_content_preview: p.content.chars().take(100).collect(),
        })
    });

    let avg_depth = if cascade_stats.is_empty() { 0.0 } else {
        cascade_stats.iter().map(|(_, _, d)| *d as f32).sum::<f32>() / cascade_stats.len() as f32
    };

    // Viral posts by engagement velocity
    let mut viral_posts: Vec<ViralPost> = state.world.posts.values()
        .filter(|p| p.reply_to.is_none() && p.repost_of.is_none())
        .filter(|p| p.is_viral(current_round))
        .map(|p| {
            let age = current_round.saturating_sub(p.created_at_round).max(1) as f64;
            ViralPost {
                post_id: p.short_id(),
                author: p.author_name.clone(),
                content_preview: p.content.chars().take(80).collect(),
                engagement: p.engagement_score(),
                velocity: p.engagement_score() / age,
            }
        })
        .collect();
    viral_posts.sort_by(|a, b| b.velocity.partial_cmp(&a.velocity).unwrap_or(std::cmp::Ordering::Equal));
    viral_posts.truncate(10);

    let total_posts = state.world.posts.values().filter(|p| p.reply_to.is_none()).count();
    let repost_count = state.world.posts.values().filter(|p| p.repost_of.is_some()).count();
    let amplification_ratio = if total_posts > 0 {
        repost_count as f32 / total_posts as f32
    } else { 0.0 };

    ViralityMetrics {
        cascade_count: cascade_stats.len(),
        largest_cascade,
        avg_cascade_depth: avg_depth,
        viral_post_count: viral_posts.len(),
        top_viral: viral_posts,
        amplification_ratio,
    }
}

fn compute_influence(state: &SimulationState) -> InfluenceMetrics {
    // Calculate per-agent total engagement
    let mut agent_engagement: Vec<(Uuid, f64)> = state.agents.keys()
        .map(|id| {
            let total: f64 = state.world.posts.values()
                .filter(|p| p.author_id == *id)
                .map(|p| p.engagement_score())
                .sum();
            (*id, total)
        })
        .collect();
    agent_engagement.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Top influencers
    let top_influencers: Vec<InfluencerInfo> = agent_engagement.iter()
        .take(10)
        .filter_map(|(id, eng)| {
            let profile = state.agents.get(id)?;
            let followers = state.world.social_graph.follower_count(id);
            Some(InfluencerInfo {
                agent_id: id.to_string(),
                username: profile.username.clone(),
                tier: profile.tier.to_string(),
                follower_count: followers,
                total_engagement: *eng,
                influence_score: *eng + followers as f64 * 2.0,
            })
        })
        .collect();

    // Gini coefficient of engagement
    let mut engagements: Vec<f64> = agent_engagement.iter().map(|(_, e)| *e).collect();
    engagements.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let engagement_gini = compute_gini(&engagements);

    // Top 10% share
    let n = engagements.len();
    let top_n = (n as f64 * 0.1).ceil() as usize;
    let total_eng: f64 = engagements.iter().sum();
    let top_eng: f64 = engagements.iter().rev().take(top_n).sum();
    let top_10_pct_share = if total_eng > 0.0 {
        (top_eng / total_eng) as f32
    } else { 0.0 };

    InfluenceMetrics {
        top_influencers,
        engagement_gini,
        top_10_pct_share,
    }
}

fn compute_gini(sorted_values: &[f64]) -> f32 {
    let n = sorted_values.len();
    if n == 0 { return 0.0; }
    let total: f64 = sorted_values.iter().sum();
    if total == 0.0 { return 0.0; }

    let mut cumulative = 0.0;
    let mut gini_sum = 0.0;
    for (i, &v) in sorted_values.iter().enumerate() {
        cumulative += v;
        gini_sum += (2.0 * (i + 1) as f64 - n as f64 - 1.0) * v;
    }
    (gini_sum / (n as f64 * total)) as f32
}

fn compute_contagion(state: &SimulationState) -> ContagionMetrics {
    // Time to peak for posts with engagement
    let current_round = state.world.current_round;
    let mut times_to_peak: Vec<u32> = Vec::new();

    for post in state.world.posts.values() {
        if post.reply_to.is_some() || post.repost_of.is_some() {
            continue;
        }
        if post.engagement_score() < 2.0 {
            continue;
        }
        // Estimate peak round as the round of the last engagement action
        let peak = post.replies.iter()
            .chain(post.likes.iter())
            .filter_map(|id| {
                state.world.posts.get(id).map(|p| p.created_at_round)
            })
            .max()
            .unwrap_or(post.created_at_round);
        let time = peak.saturating_sub(post.created_at_round);
        if time > 0 {
            times_to_peak.push(time);
        }
    }

    let avg_time_to_peak = if times_to_peak.is_empty() { 0.0 } else {
        times_to_peak.iter().sum::<u32>() as f32 / times_to_peak.len() as f32
    };

    // Topic persistence: track keywords across rounds
    let topic_words = [
        "ai", "automation", "layoffs", "jobs", "workers", "technology",
        "regulation", "safety", "privacy", "economy",
    ];
    let mut topic_rounds: HashMap<&str, (u32, u32, usize)> = HashMap::new();

    for post in state.world.posts.values() {
        let lower = post.content.to_lowercase();
        for &topic in &topic_words {
            if lower.contains(topic) {
                let entry = topic_rounds.entry(topic).or_insert((post.created_at_round, post.created_at_round, 0));
                entry.0 = entry.0.min(post.created_at_round);
                entry.1 = entry.1.max(post.created_at_round);
                entry.2 += 1;
            }
        }
    }

    let mut topic_persistence: Vec<TopicPersistence> = topic_rounds.into_iter()
        .filter(|(_, (_, _, count))| *count >= 2)
        .map(|(topic, (first, last, count))| TopicPersistence {
            topic: topic.to_string(),
            first_round: first,
            last_round: last,
            duration: last.saturating_sub(first) + 1,
            post_count: count,
        })
        .collect();
    topic_persistence.sort_by(|a, b| b.post_count.cmp(&a.post_count));

    ContagionMetrics {
        avg_time_to_peak,
        fastest_spread: None, // Could be computed but expensive
        topic_persistence,
    }
}

fn compute_community(state: &SimulationState) -> CommunityMetrics {
    let n = state.agents.len();

    // Isolated agents
    let isolated = state.agents.keys()
        .filter(|id| {
            let followers = state.world.social_graph.follower_count(id);
            let following = state.world.social_graph.following.get(id).map_or(0, |f| f.len());
            followers == 0 && following == 0
        })
        .count();

    // Graph density
    let total_edges: usize = state.world.social_graph.following.values().map(|f| f.len()).sum();
    let possible_edges = n * (n.saturating_sub(1));
    let density = if possible_edges > 0 {
        total_edges as f32 / possible_edges as f32
    } else { 0.0 };

    // Cross-stance interactions: count replies between agents of opposing stances
    let mut cross_stance = 0usize;
    let mut same_stance = 0usize;

    for post in state.world.posts.values() {
        if post.reply_to.is_none() { continue; }
        if let Some(parent_id) = post.reply_to {
            if let Some(parent) = state.world.posts.get(&parent_id) {
                let author_stance = state.agents.get(&post.author_id).map(|a| a.stance);
                let parent_stance = state.agents.get(&parent.author_id).map(|a| a.stance);
                if let (Some(a), Some(b)) = (author_stance, parent_stance) {
                    if a == b {
                        same_stance += 1;
                    } else {
                        cross_stance += 1;
                    }
                }
            }
        }
    }

    let echo_chamber_score = if same_stance + cross_stance > 0 {
        same_stance as f32 / (same_stance + cross_stance) as f32
    } else { 0.5 };

    // Simple connected components via BFS
    let agent_ids: Vec<Uuid> = state.agents.keys().cloned().collect();
    let mut visited: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    let mut components = 0;

    for &id in &agent_ids {
        if visited.contains(&id) { continue; }
        components += 1;
        let mut queue = vec![id];
        while let Some(current) = queue.pop() {
            if !visited.insert(current) { continue; }
            // Follow edges in both directions
            if let Some(following) = state.world.social_graph.following.get(&current) {
                for &target in following {
                    if !visited.contains(&target) { queue.push(target); }
                }
            }
            if let Some(followers) = state.world.social_graph.followers.get(&current) {
                for &source in followers {
                    if !visited.contains(&source) { queue.push(source); }
                }
            }
        }
    }

    CommunityMetrics {
        components,
        isolated_agents: isolated,
        avg_clustering: 0.0, // Expensive to compute, skip for now
        density,
        cross_stance_interactions: cross_stance,
        echo_chamber_score,
    }
}

fn compute_content(state: &SimulationState) -> ContentMetrics {
    let mut post_count = 0;
    let mut reply_count = 0;
    let mut repost_count = 0;
    let mut quote_count = 0;
    let mut contested_count = 0;
    let mut total_length: usize = 0;
    let mut hashtag_counts: HashMap<String, usize> = HashMap::new();

    for post in state.world.posts.values() {
        if post.reply_to.is_some() {
            reply_count += 1;
        } else if post.repost_of.is_some() {
            repost_count += 1;
        } else if post.quote_of.is_some() {
            quote_count += 1;
        } else {
            post_count += 1;
        }

        if post.is_contested() {
            contested_count += 1;
        }

        total_length += post.content.len();

        for tag in &post.hashtags {
            *hashtag_counts.entry(tag.clone()).or_insert(0) += 1;
        }
    }

    let total = state.world.posts.len();
    let avg_post_length = if total > 0 { total_length as f32 / total as f32 } else { 0.0 };

    let mut top_hashtags: Vec<(String, usize)> = hashtag_counts.into_iter().collect();
    top_hashtags.sort_by(|a, b| b.1.cmp(&a.1));
    top_hashtags.truncate(10);

    ContentMetrics {
        post_count,
        reply_count,
        repost_count,
        quote_count,
        contested_count,
        avg_post_length,
        top_hashtags,
    }
}

fn compute_cognitive(state: &SimulationState) -> CognitiveMetrics {
    let agents: Vec<_> = state.agent_states.values().collect();
    let n = agents.len() as f32;
    if n == 0.0 {
        return CognitiveMetrics {
            avg_fatigue: 0.0,
            exhausted_count: 0,
            avg_belief_strength: 0.0,
            controversial_topics: Vec::new(),
        };
    }

    let avg_fatigue = agents.iter().map(|a| a.cognitive.fatigue).sum::<f32>() / n;
    let exhausted_count = agents.iter().filter(|a| a.cognitive.fatigue > 0.7).count();

    // Average belief strength
    let total_strength: f32 = agents.iter()
        .flat_map(|a| a.beliefs.values())
        .map(|v| v.abs())
        .sum();
    let total_beliefs = agents.iter().map(|a| a.beliefs.len()).sum::<usize>();
    let avg_belief_strength = if total_beliefs > 0 {
        total_strength / total_beliefs as f32
    } else { 0.0 };

    // Controversial topics: collect all beliefs and compute variance per topic
    let mut topic_values: HashMap<String, Vec<f32>> = HashMap::new();
    for agent in &agents {
        for (topic, &value) in &agent.beliefs {
            topic_values.entry(topic.clone()).or_default().push(value);
        }
    }

    let mut controversial_topics: Vec<(String, f32)> = topic_values.iter()
        .filter(|(_, vals)| vals.len() >= 3)
        .map(|(topic, vals)| {
            let mean = vals.iter().sum::<f32>() / vals.len() as f32;
            let variance = vals.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / vals.len() as f32;
            (topic.clone(), variance)
        })
        .collect();
    controversial_topics.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    controversial_topics.truncate(10);

    CognitiveMetrics {
        avg_fatigue,
        exhausted_count,
        avg_belief_strength,
        controversial_topics,
    }
}
