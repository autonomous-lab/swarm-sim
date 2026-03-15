use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agent::{Stance, Tier};

// ---------------------------------------------------------------------------
// Action types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    CreatePost,
    Reply,
    Like,
    Repost,
    Follow,
    Unfollow,
    DoNothing,
    PinMemory,
}

impl std::fmt::Display for ActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionType::CreatePost => write!(f, "create_post"),
            ActionType::Reply => write!(f, "reply"),
            ActionType::Like => write!(f, "like"),
            ActionType::Repost => write!(f, "repost"),
            ActionType::Follow => write!(f, "follow"),
            ActionType::Unfollow => write!(f, "unfollow"),
            ActionType::DoNothing => write!(f, "do_nothing"),
            ActionType::PinMemory => write!(f, "pin_memory"),
        }
    }
}

// ---------------------------------------------------------------------------
// Action
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub id: Uuid,
    pub round: u32,
    pub simulated_time: DateTime<Utc>,
    pub agent_id: Uuid,
    pub agent_name: String,
    pub agent_tier: Tier,
    pub action_type: ActionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_post_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_agent_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

// ---------------------------------------------------------------------------
// Post
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: Uuid,
    pub author_id: Uuid,
    pub author_name: String,
    pub content: String,
    pub created_at_round: u32,
    pub simulated_time: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repost_of: Option<Uuid>,
    pub likes: Vec<Uuid>,
    pub replies: Vec<Uuid>,
    pub reposts: Vec<Uuid>,
    pub hashtags: Vec<String>,
}

impl Post {
    pub fn engagement_score(&self) -> f64 {
        self.likes.len() as f64 + self.replies.len() as f64 * 2.0 + self.reposts.len() as f64 * 3.0
    }

    /// Short ID for display (first 8 chars of UUID).
    pub fn short_id(&self) -> String {
        self.id.to_string()[..8].to_string()
    }
}

// ---------------------------------------------------------------------------
// Social graph
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SocialGraph {
    pub followers: HashMap<Uuid, Vec<Uuid>>,
    pub following: HashMap<Uuid, Vec<Uuid>>,
}

impl SocialGraph {
    pub fn add_follow(&mut self, follower: Uuid, target: Uuid) {
        self.following.entry(follower).or_default().push(target);
        self.followers.entry(target).or_default().push(follower);
    }

    pub fn remove_follow(&mut self, follower: Uuid, target: Uuid) {
        if let Some(list) = self.following.get_mut(&follower) {
            list.retain(|&id| id != target);
        }
        if let Some(list) = self.followers.get_mut(&target) {
            list.retain(|&id| id != follower);
        }
    }

    pub fn is_following(&self, follower: &Uuid, target: &Uuid) -> bool {
        self.following
            .get(follower)
            .map_or(false, |list| list.contains(target))
    }

    pub fn follower_count(&self, agent: &Uuid) -> usize {
        self.followers.get(agent).map_or(0, |l| l.len())
    }
}

// ---------------------------------------------------------------------------
// Injected events (God's Eye)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InjectedEventType {
    BreakingNews,
    AgentMood,
    ViralContent,
    RemoveAgent,
    AddAgent,
    SystemAnnouncement,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectedEvent {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inject_at_round: Option<u32>,
    pub event_type: InjectedEventType,
    pub content: String,
    #[serde(default)]
    pub processed: bool,
}

// ---------------------------------------------------------------------------
// Round summary (for timeline API)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundSummary {
    pub round: u32,
    pub simulated_time: DateTime<Utc>,
    pub active_agents: usize,
    pub total_actions: usize,
    pub new_posts: usize,
    pub new_replies: usize,
    pub new_likes: usize,
    pub new_reposts: usize,
    pub new_follows: usize,
    pub events_injected: usize,
}

// ---------------------------------------------------------------------------
// World state
// ---------------------------------------------------------------------------

/// Maximum number of posts before old ones get pruned.
const MAX_POSTS: usize = 50_000;
/// When pruning, keep this many most-recent posts.
const PRUNE_KEEP: usize = 40_000;

#[derive(Debug, Clone)]
pub struct WorldState {
    pub posts: HashMap<Uuid, Post>,
    pub post_timeline: Vec<Uuid>,
    pub social_graph: SocialGraph,
    pub current_round: u32,
    pub simulated_time: DateTime<Utc>,
    pub injected_events: Vec<InjectedEvent>,
    pub round_summaries: Vec<RoundSummary>,
}

impl WorldState {
    pub fn new(start_time: DateTime<Utc>) -> Self {
        Self {
            posts: HashMap::new(),
            post_timeline: Vec::new(),
            social_graph: SocialGraph::default(),
            current_round: 0,
            simulated_time: start_time,
            injected_events: Vec::new(),
            round_summaries: Vec::new(),
        }
    }

    /// Prune oldest posts if we exceed MAX_POSTS.
    fn prune_if_needed(&mut self) {
        if self.posts.len() <= MAX_POSTS {
            return;
        }
        let remove_count = self.post_timeline.len().saturating_sub(PRUNE_KEEP);
        let to_remove: Vec<Uuid> = self.post_timeline.drain(..remove_count).collect();
        for id in &to_remove {
            self.posts.remove(id);
        }
        tracing::info!("Pruned {} old posts (kept {})", to_remove.len(), self.posts.len());
    }

    /// Build a personalized feed for an agent.
    pub fn build_feed(
        &self,
        agent_id: &Uuid,
        feed_size: usize,
        recency_w: f32,
        popularity_w: f32,
        relevance_w: f32,
    ) -> Vec<&Post> {
        let following = self
            .social_graph
            .following
            .get(agent_id)
            .cloned()
            .unwrap_or_default();

        let mut scored: Vec<(&Post, f64)> = self
            .posts
            .values()
            .filter(|p| p.author_id != *agent_id)
            .map(|post| {
                let age = (self.current_round.saturating_sub(post.created_at_round)) as f64;
                let recency = 1.0 / (1.0 + age);
                let engagement = post.engagement_score() / 100.0;
                let followed = if following.contains(&post.author_id) {
                    1.0
                } else {
                    0.2 // Non-followed content still gets baseline visibility
                };

                let score = recency_w as f64 * recency
                    + popularity_w as f64 * engagement
                    + relevance_w as f64 * followed;

                (post, score)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(feed_size);
        scored.into_iter().map(|(p, _)| p).collect()
    }

    /// Build reply candidates: posts the agent should consider engaging with.
    /// Returns posts that are stance-opposing, viral, or from followed users.
    pub fn build_reply_candidates(
        &self,
        agent_id: &Uuid,
        agent_stance: &Stance,
        max_candidates: usize,
    ) -> Vec<ReplyCandidate> {
        let following = self
            .social_graph
            .following
            .get(agent_id)
            .cloned()
            .unwrap_or_default();

        let recent_posts: Vec<&Post> = self
            .posts
            .values()
            .filter(|p| {
                p.author_id != *agent_id
                    && p.reply_to.is_none()
                    && self.current_round.saturating_sub(p.created_at_round) <= 3
            })
            .collect();

        let mut candidates: Vec<ReplyCandidate> = Vec::new();

        for post in &recent_posts {
            let content_lower = post.content.to_lowercase();

            // Check stance opposition using keyword heuristics
            let is_opposing = match agent_stance {
                Stance::Supportive => {
                    content_lower.contains("greed") || content_lower.contains("scam")
                        || content_lower.contains("disaster") || content_lower.contains("betrayal")
                        || content_lower.contains("terrible") || content_lower.contains("worst")
                        || content_lower.contains("outrage") || content_lower.contains("unacceptable")
                }
                Stance::Opposing => {
                    content_lower.contains("innovation") || content_lower.contains("progress")
                        || content_lower.contains("opportunity") || content_lower.contains("great")
                        || content_lower.contains("exciting") || content_lower.contains("brilliant")
                        || content_lower.contains("impressive") || content_lower.contains("support")
                }
                _ => false,
            };

            let is_viral = post.engagement_score() > 5.0;
            let is_followed = following.contains(&post.author_id);
            let has_few_replies = post.replies.len() < 3;

            if is_opposing {
                candidates.push(ReplyCandidate {
                    post_id: post.id,
                    author_name: post.author_name.clone(),
                    content_preview: post.content.chars().take(100).collect(),
                    engagement: post.engagement_score(),
                    reason: "DISAGREES with your stance. Push back or engage.".into(),
                });
            } else if is_viral {
                candidates.push(ReplyCandidate {
                    post_id: post.id,
                    author_name: post.author_name.clone(),
                    content_preview: post.content.chars().take(100).collect(),
                    engagement: post.engagement_score(),
                    reason: "VIRAL post. Engaging increases your visibility.".into(),
                });
            } else if is_followed && has_few_replies {
                candidates.push(ReplyCandidate {
                    post_id: post.id,
                    author_name: post.author_name.clone(),
                    content_preview: post.content.chars().take(100).collect(),
                    engagement: post.engagement_score(),
                    reason: "From someone you follow. Join the conversation.".into(),
                });
            }
        }

        // Sort: opposing first, then viral, then followed
        candidates.sort_by(|a, b| {
            let a_priority = if a.reason.contains("DISAGREES") { 0 }
                else if a.reason.contains("VIRAL") { 1 }
                else { 2 };
            let b_priority = if b.reason.contains("DISAGREES") { 0 }
                else if b.reason.contains("VIRAL") { 1 }
                else { 2 };
            a_priority.cmp(&b_priority)
                .then(b.engagement.partial_cmp(&a.engagement).unwrap_or(std::cmp::Ordering::Equal))
        });

        candidates.truncate(max_candidates);
        candidates
    }

    /// Top posts by engagement in last `lookback` rounds.
    pub fn trending(&self, count: usize, lookback: u32) -> Vec<&Post> {
        let min_round = self.current_round.saturating_sub(lookback);
        let mut posts: Vec<&Post> = self
            .posts
            .values()
            .filter(|p| p.created_at_round >= min_round && p.reply_to.is_none())
            .collect();
        posts.sort_by(|a, b| {
            b.engagement_score()
                .partial_cmp(&a.engagement_score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        posts.truncate(count);
        posts
    }

    /// Add a new post to the world.
    pub fn add_post(&mut self, post: Post) {
        let id = post.id;
        if let Some(reply_to) = post.reply_to {
            if let Some(parent) = self.posts.get_mut(&reply_to) {
                parent.replies.push(id);
            }
        }
        self.post_timeline.push(id);
        self.posts.insert(id, post);
        self.prune_if_needed();
    }

    /// Record a like.
    pub fn add_like(&mut self, post_id: Uuid, agent_id: Uuid) {
        if let Some(post) = self.posts.get_mut(&post_id) {
            if !post.likes.contains(&agent_id) {
                post.likes.push(agent_id);
            }
        }
    }

    /// Record a repost.
    pub fn add_repost(&mut self, original_id: Uuid, repost: Post) {
        let repost_id = repost.id;
        self.post_timeline.push(repost_id);
        self.posts.insert(repost_id, repost);
        if let Some(original) = self.posts.get_mut(&original_id) {
            original.reposts.push(repost_id);
        }
    }
}

// ---------------------------------------------------------------------------
// Reply candidate for discourse injection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyCandidate {
    pub post_id: Uuid,
    pub author_name: String,
    pub content_preview: String,
    pub engagement: f64,
    pub reason: String,
}
