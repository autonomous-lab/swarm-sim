use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::{self, CorsLayer};
use tower_http::services::ServeDir;
use uuid::Uuid;

use crate::agent::Tier;
use crate::config::SimConfig;
use crate::engine::{EngineControls, SharedState, SimStatus, WsEvent};
use crate::launcher::{self, LaunchRequest};
use crate::llm::{LlmClient, PostSummary};
use crate::world::InjectedEvent;

// ---------------------------------------------------------------------------
// App state for Axum
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AppState {
    pub sim_state: SharedState,
    pub controls: Arc<RwLock<EngineControls>>,
    pub ws_tx: broadcast::Sender<WsEvent>,
    pub llm: Arc<LlmClient>,
    pub base_config: Arc<SimConfig>,
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn create_router(app_state: AppState, web_dir: &str, cors_permissive: bool) -> Router {
    let cors = if cors_permissive {
        CorsLayer::permissive()
    } else {
        CorsLayer::new()
            .allow_origin(cors::Any)
            .allow_methods([
                axum::http::Method::GET,
                axum::http::Method::POST,
            ])
            .allow_headers([axum::http::header::CONTENT_TYPE])
    };

    Router::new()
        // REST API
        .route("/api/status", get(get_status))
        .route("/api/agents", get(get_agents))
        .route("/api/agents/{id}", get(get_agent))
        .route("/api/posts", get(get_posts))
        .route("/api/posts/{id}", get(get_post))
        .route("/api/trending", get(get_trending))
        .route("/api/timeline", get(get_timeline))
        .route("/api/graph", get(get_graph))
        .route("/api/dashboard", get(get_dashboard))
        .route("/api/solutions", get(get_solutions))
        .route("/api/syntheses", get(get_syntheses))
        .route("/api/sentiment-timeline", get(get_sentiment_timeline))
        .route("/api/simulation/pause", post(pause_simulation))
        .route("/api/simulation/resume", post(resume_simulation))
        .route("/api/simulation/stop", post(stop_simulation))
        .route("/api/simulation/launch", post(launch_simulation))
        .route("/api/simulation/continue", post(continue_simulation))
        .route("/api/simulation/save", post(save_state))
        .route("/api/simulation/load", post(load_state))
        .route("/api/god-eye/inject", post(inject_event))
        .route("/api/metrics", get(get_metrics))
        .route("/api/metrics/polarization", get(get_polarization))
        .route("/api/metrics/virality", get(get_virality))
        .route("/api/metrics/influence", get(get_influence))
        .route("/api/metrics/cascades", get(get_cascades))
        .route("/api/metrics/community", get(get_community_metrics))
        .route("/api/metrics/contagion", get(get_contagion))
        .route("/api/metrics/cognitive", get(get_cognitive_metrics))
        .route("/api/metrics/compare", post(compare_runs))
        .route("/api/validate", get(validate_state_endpoint))
        .route("/api/export/json", get(export_json))
        .route("/api/export/metrics", get(export_metrics_json))
        // WebSocket
        .route("/ws", get(ws_handler))
        // Static files (web UI)
        .fallback_service(ServeDir::new(web_dir))
        .layer(cors)
        .with_state(app_state)
}

/// Start the web server.
pub async fn start(
    host: &str,
    port: u16,
    app_state: AppState,
    web_dir: &str,
    cors_permissive: bool,
) -> anyhow::Result<()> {
    let router = create_router(app_state, web_dir, cors_permissive);
    let addr = format!("{host}:{port}");
    tracing::info!("Web UI: http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, router).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// REST handlers
// ---------------------------------------------------------------------------

async fn get_status(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;
    Json(s.snapshot())
}

#[derive(Serialize)]
struct AgentSummary {
    id: String,
    name: String,
    username: String,
    tier: Tier,
    bio: String,
    post_count: usize,
    follower_count: usize,
    stance: String,
}

async fn get_agents(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;
    let mut agents: Vec<AgentSummary> = s
        .agents
        .values()
        .map(|a| {
            let post_count = s
                .agent_states
                .get(&a.id)
                .map(|st| st.post_ids.len())
                .unwrap_or(0);
            let follower_count = s.world.social_graph.follower_count(&a.id);
            AgentSummary {
                id: a.id.to_string(),
                name: a.name.clone(),
                username: a.username.clone(),
                tier: a.tier,
                bio: a.bio.clone(),
                post_count,
                follower_count,
                stance: a.stance.to_string(),
            }
        })
        .collect();

    agents.sort_by(|a, b| {
        a.tier
            .to_string()
            .cmp(&b.tier.to_string())
            .then(b.follower_count.cmp(&a.follower_count))
    });

    Json(agents)
}

#[derive(Serialize)]
struct AgentDetail {
    profile: crate::agent::AgentProfile,
    state: Option<crate::agent::AgentState>,
    recent_posts: Vec<crate::world::Post>,
}

async fn get_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let agent_id = match Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid UUID"}))).into_response(),
    };

    let s = state.sim_state.read().await;
    let profile = match s.agents.get(&agent_id) {
        Some(p) => p.clone(),
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Agent not found"}))).into_response(),
    };
    let agent_state = s.agent_states.get(&agent_id).cloned();
    let recent_posts: Vec<_> = s
        .world
        .posts
        .values()
        .filter(|p| p.author_id == agent_id)
        .cloned()
        .collect();

    Json(AgentDetail {
        profile,
        state: agent_state,
        recent_posts,
    })
    .into_response()
}

#[derive(Deserialize)]
struct PaginationParams {
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    tier: Option<String>,
}

fn default_limit() -> usize { 50 }

async fn get_posts(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> impl IntoResponse {
    let s = state.sim_state.read().await;
    let mut posts: Vec<_> = s.world.posts.values().cloned().collect();
    posts.sort_by(|a, b| b.created_at_round.cmp(&a.created_at_round));

    if let Some(tier_filter) = &params.tier {
        posts.retain(|p| {
            s.agents
                .get(&p.author_id)
                .map(|a| a.tier.to_string() == *tier_filter)
                .unwrap_or(false)
        });
    }

    let total = posts.len();
    let posts: Vec<_> = posts.into_iter().skip(params.offset).take(params.limit).collect();

    Json(serde_json::json!({
        "total": total,
        "posts": posts,
    }))
}

async fn get_post(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let post_id = match Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid UUID"}))).into_response(),
    };

    let s = state.sim_state.read().await;
    match s.world.posts.get(&post_id) {
        Some(post) => {
            let replies: Vec<_> = post
                .replies
                .iter()
                .filter_map(|rid| s.world.posts.get(rid).cloned())
                .collect();
            Json(serde_json::json!({
                "post": post,
                "replies": replies,
            }))
            .into_response()
        }
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Post not found"}))).into_response(),
    }
}

async fn get_trending(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;
    let trending = s.world.trending(10, 10);
    let summaries: Vec<PostSummary> = trending
        .iter()
        .map(|p| PostSummary::from_post(p, s.world.current_round, 200))
        .collect();
    Json(summaries)
}

async fn get_timeline(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;
    Json(s.world.round_summaries.clone())
}

#[derive(Serialize)]
struct GraphData {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
}

#[derive(Serialize)]
struct GraphNode {
    id: String,
    label: String,
    tier: String,
    size: f32,
    post_count: usize,
    follower_count: usize,
    following_count: usize,
    stance: String,
    sentiment: f32,
}

#[derive(Serialize)]
struct GraphEdge {
    source: String,
    target: String,
}

async fn get_graph(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;

    let nodes: Vec<GraphNode> = s
        .agents
        .values()
        .map(|a| {
            let agent_state = s.agent_states.get(&a.id);
            let post_count = agent_state.map(|st| st.post_ids.len()).unwrap_or(0);
            let follower_count = agent_state.map(|st| st.followers.len()).unwrap_or(0);
            let following_count = agent_state.map(|st| st.following.len()).unwrap_or(0);
            GraphNode {
                id: a.id.to_string(),
                label: format!("@{}", a.username),
                tier: a.tier.to_string(),
                size: match a.tier {
                    Tier::Tier1 => 20.0,
                    Tier::Tier2 => 12.0,
                    Tier::Tier3 => 6.0,
                },
                post_count,
                follower_count,
                following_count,
                stance: a.stance.to_string(),
                sentiment: a.sentiment_bias,
            }
        })
        .collect();

    let mut edges = Vec::new();
    for (follower, targets) in &s.world.social_graph.following {
        for target in targets {
            edges.push(GraphEdge {
                source: follower.to_string(),
                target: target.to_string(),
            });
        }
    }

    Json(GraphData { nodes, edges })
}

// ---------------------------------------------------------------------------
// Dashboard endpoint
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct DashboardData {
    stance_distribution: HashMap<String, usize>,
    tier_distribution: HashMap<String, usize>,
    activity_per_round: Vec<RoundActivity>,
    top_agents: Vec<TopAgent>,
    total_posts: usize,
    total_actions: usize,
    total_agents: usize,
}

#[derive(Serialize)]
struct RoundActivity {
    round: u32,
    posts: usize,
    replies: usize,
    likes: usize,
    active_agents: usize,
}

#[derive(Serialize)]
struct TopAgent {
    username: String,
    tier: String,
    post_count: usize,
    follower_count: usize,
    stance: String,
}

async fn get_dashboard(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;

    let mut stance_distribution: HashMap<String, usize> = HashMap::new();
    let mut tier_distribution: HashMap<String, usize> = HashMap::new();

    for agent in s.agents.values() {
        *stance_distribution.entry(agent.stance.to_string()).or_insert(0) += 1;
        let tier_label = match agent.tier {
            Tier::Tier1 => "VIP",
            Tier::Tier2 => "Standard",
            Tier::Tier3 => "Figurant",
        };
        *tier_distribution.entry(tier_label.to_string()).or_insert(0) += 1;
    }

    let activity_per_round: Vec<RoundActivity> = s
        .world
        .round_summaries
        .iter()
        .map(|rs| RoundActivity {
            round: rs.round,
            posts: rs.new_posts,
            replies: rs.new_replies,
            likes: rs.new_likes,
            active_agents: rs.active_agents,
        })
        .collect();

    let mut top_agents: Vec<TopAgent> = s
        .agents
        .values()
        .map(|a| {
            let post_count = s
                .agent_states
                .get(&a.id)
                .map(|st| st.post_ids.len())
                .unwrap_or(0);
            let follower_count = s.world.social_graph.follower_count(&a.id);
            TopAgent {
                username: a.username.clone(),
                tier: match a.tier {
                    Tier::Tier1 => "VIP".to_string(),
                    Tier::Tier2 => "Standard".to_string(),
                    Tier::Tier3 => "Figurant".to_string(),
                },
                post_count,
                follower_count,
                stance: a.stance.to_string(),
            }
        })
        .collect();
    top_agents.sort_by(|a, b| (b.post_count + b.follower_count).cmp(&(a.post_count + a.follower_count)));
    top_agents.truncate(10);

    Json(DashboardData {
        stance_distribution,
        tier_distribution,
        activity_per_round,
        top_agents,
        total_posts: s.world.posts.len(),
        total_actions: s.total_actions,
        total_agents: s.agents.len(),
    })
}

// ---------------------------------------------------------------------------
// Solutions endpoint
// ---------------------------------------------------------------------------

async fn get_solutions(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;
    let mut solutions: Vec<serde_json::Value> = s
        .world
        .solution_ids
        .iter()
        .filter_map(|id| s.world.posts.get(id))
        .map(|p| {
            let votes = s.world.solution_votes.get(&p.id).map(|v| v.len()).unwrap_or(0);
            let refines_of = p.refines.map(|r| r.to_string());
            // Find refinements of this solution
            let refinements: Vec<String> = s.world.solution_ids.iter()
                .filter_map(|sid| {
                    let sp = s.world.posts.get(sid)?;
                    if sp.refines == Some(p.id) { Some(sid.to_string()) } else { None }
                })
                .collect();
            serde_json::json!({
                "id": p.id,
                "author_name": p.author_name,
                "content": p.content,
                "created_at_round": p.created_at_round,
                "likes": p.likes.len(),
                "replies": p.replies.len(),
                "reposts": p.reposts.len(),
                "engagement": p.engagement_score(),
                "votes": votes,
                "refines_of": refines_of,
                "refinements": refinements,
            })
        })
        .collect();
    solutions.sort_by(|a, b| {
        let a_score = a["votes"].as_u64().unwrap_or(0) as f64 * 10.0 + a["engagement"].as_f64().unwrap_or(0.0);
        let b_score = b["votes"].as_u64().unwrap_or(0) as f64 * 10.0 + b["engagement"].as_f64().unwrap_or(0.0);
        b_score.partial_cmp(&a_score).unwrap_or(std::cmp::Ordering::Equal)
    });
    let challenge = s.config.simulation.challenge_question.clone();
    Json(serde_json::json!({
        "challenge_question": challenge,
        "solutions": solutions,
    }))
}

// ---------------------------------------------------------------------------
// Sentiment timeline endpoint
// ---------------------------------------------------------------------------

async fn get_sentiment_timeline(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;

    // Build per-round average sentiment by stance
    let max_round = s.world.current_round;
    let mut timeline: Vec<serde_json::Value> = Vec::new();

    for round in 1..=max_round {
        let mut supportive_sum = 0.0f32;
        let mut supportive_count = 0u32;
        let mut opposing_sum = 0.0f32;
        let mut opposing_count = 0u32;
        let mut neutral_sum = 0.0f32;
        let mut neutral_count = 0u32;

        for (agent_id, agent_state) in &s.agent_states {
            let stance = s.agents.get(agent_id).map(|a| a.stance).unwrap_or(crate::agent::Stance::Neutral);
            // Find the sentiment value closest to this round
            let sentiment = agent_state.sentiment_history.iter()
                .filter(|(r, _)| *r <= round)
                .last()
                .map(|(_, v)| *v)
                .unwrap_or(agent_state.current_sentiment);

            match stance {
                crate::agent::Stance::Supportive => { supportive_sum += sentiment; supportive_count += 1; }
                crate::agent::Stance::Opposing => { opposing_sum += sentiment; opposing_count += 1; }
                _ => { neutral_sum += sentiment; neutral_count += 1; }
            }
        }

        timeline.push(serde_json::json!({
            "round": round,
            "supportive": if supportive_count > 0 { supportive_sum / supportive_count as f32 } else { 0.0 },
            "opposing": if opposing_count > 0 { opposing_sum / opposing_count as f32 } else { 0.0 },
            "neutral": if neutral_count > 0 { neutral_sum / neutral_count as f32 } else { 0.0 },
        }));
    }

    Json(timeline)
}

// ---------------------------------------------------------------------------
// Syntheses endpoint
// ---------------------------------------------------------------------------

async fn get_syntheses(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;
    let syntheses: Vec<serde_json::Value> = s
        .syntheses
        .iter()
        .map(|(round, text)| {
            serde_json::json!({ "round": round, "text": text })
        })
        .collect();
    Json(syntheses)
}

// ---------------------------------------------------------------------------
// Simulation control
// ---------------------------------------------------------------------------

async fn pause_simulation(State(state): State<AppState>) -> impl IntoResponse {
    let controls = state.controls.read().await;
    let _ = controls.pause_tx.send(()).await;
    Json(serde_json::json!({"status": "paused"}))
}

async fn resume_simulation(State(state): State<AppState>) -> impl IntoResponse {
    let controls = state.controls.read().await;
    let _ = controls.resume_tx.send(()).await;
    Json(serde_json::json!({"status": "resumed"}))
}

async fn stop_simulation(State(state): State<AppState>) -> impl IntoResponse {
    let controls = state.controls.read().await;
    let _ = controls.stop_tx.send(()).await;
    Json(serde_json::json!({"status": "stopping"}))
}

// ---------------------------------------------------------------------------
// Launch simulation from UI
// ---------------------------------------------------------------------------

async fn launch_simulation(
    State(state): State<AppState>,
    Json(req): Json<LaunchRequest>,
) -> impl IntoResponse {
    // Check current status
    {
        let s = state.sim_state.read().await;
        match s.status {
            SimStatus::Running | SimStatus::Preparing => {
                return (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({"error": "Simulation already running. Stop it first."})),
                )
                    .into_response();
            }
            _ => {}
        }
    }

    // Validate request
    if req.scenario_prompt.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "scenario_prompt is required"})),
        )
            .into_response();
    }

    // Set status to Preparing immediately so the UI can show a loading state
    {
        let mut s = state.sim_state.write().await;
        s.status = SimStatus::Preparing;
    }

    // Spawn agent generation + engine startup in background — return 202 immediately
    let bg_state = state.clone();
    tokio::spawn(async move {
        match launcher::launch_simulation(
            req,
            &bg_state.base_config,
            bg_state.llm.clone(),
            bg_state.sim_state.clone(),
            bg_state.ws_tx.clone(),
        )
        .await
        {
            Ok(new_controls) => {
                let mut controls = bg_state.controls.write().await;
                *controls = new_controls;
                tracing::info!("Background launch completed successfully");
            }
            Err(e) => {
                tracing::error!("Background launch failed: {e}");
                // Reset status back to idle on failure
                let mut s = bg_state.sim_state.write().await;
                s.status = SimStatus::Idle;
            }
        }
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({"status": "preparing"})),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Continue simulation
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ContinueRequest {
    extra_rounds: u32,
}

async fn continue_simulation(
    State(state): State<AppState>,
    Json(req): Json<ContinueRequest>,
) -> impl IntoResponse {
    // Must be finished to continue
    {
        let s = state.sim_state.read().await;
        match s.status {
            SimStatus::Finished => {}
            SimStatus::Running | SimStatus::Preparing => {
                return (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({"error": "Simulation is still running."})),
                )
                    .into_response();
            }
            _ => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "No finished simulation to continue."})),
                )
                    .into_response();
            }
        }
    }

    if req.extra_rounds == 0 || req.extra_rounds > 100 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "extra_rounds must be between 1 and 100"})),
        )
            .into_response();
    }

    match launcher::continue_simulation(
        req.extra_rounds,
        state.sim_state.clone(),
        state.llm.clone(),
        state.ws_tx.clone(),
    )
    .await
    {
        Ok(new_controls) => {
            let mut controls = state.controls.write().await;
            *controls = new_controls;
            Json(serde_json::json!({"status": "continuing", "extra_rounds": req.extra_rounds}))
                .into_response()
        }
        Err(e) => {
            tracing::error!("Continue failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Continue failed: {e}")})),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// God's Eye injection via API
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct InjectRequest {
    event_type: String,
    content: String,
    #[serde(default)]
    inject_at_round: Option<u32>,
}

async fn inject_event(
    State(state): State<AppState>,
    Json(req): Json<InjectRequest>,
) -> impl IntoResponse {
    if req.content.len() > 10_000 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Event content too large (max 10000 chars)"})),
        )
            .into_response();
    }

    let event_type = match req.event_type.as_str() {
        "breaking_news" => crate::world::InjectedEventType::BreakingNews,
        "agent_mood" => crate::world::InjectedEventType::AgentMood,
        "viral_content" => crate::world::InjectedEventType::ViralContent,
        "system_announcement" => crate::world::InjectedEventType::SystemAnnouncement,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid event_type"})),
            )
                .into_response()
        }
    };

    let event = InjectedEvent {
        id: format!("api-{}", Uuid::new_v4()),
        inject_at_round: req.inject_at_round,
        event_type,
        content: req.content,
        processed: false,
    };

    let controls = state.controls.read().await;
    match controls.god_eye_tx.send(event.clone()).await {
        Ok(_) => Json(serde_json::json!({"status": "injected", "event_id": event.id})).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to inject event"})),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Save / Load state
// ---------------------------------------------------------------------------

async fn save_state(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;
    let save_path = s.config.output.output_dir.join("state.json");
    match s.save_to_file(&save_path) {
        Ok(_) => Json(serde_json::json!({
            "status": "saved",
            "path": save_path.to_string_lossy(),
        }))
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Save failed: {e}")})),
        )
            .into_response(),
    }
}

async fn load_state(State(state): State<AppState>) -> impl IntoResponse {
    // Look for state.json in the output dir
    let save_path = {
        let s = state.sim_state.read().await;
        s.config.output.output_dir.join("state.json")
    };

    if !save_path.exists() {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "No saved state found. Run a simulation first, then save."})),
        )
            .into_response();
    }

    match crate::engine::SimulationState::load_from_file(&save_path) {
        Ok(loaded) => {
            let mut s = state.sim_state.write().await;
            *s = loaded;
            Json(serde_json::json!({
                "status": "loaded",
                "current_round": s.world.current_round,
                "total_agents": s.agents.len(),
                "total_posts": s.world.posts.len(),
            }))
            .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Load failed: {e}")})),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Metrics endpoints
// ---------------------------------------------------------------------------

async fn get_metrics(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;
    let metrics = crate::metrics::compute_metrics(&s);
    Json(metrics)
}

async fn get_polarization(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;
    let metrics = crate::metrics::compute_metrics(&s);
    Json(metrics.polarization)
}

async fn get_virality(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;
    let metrics = crate::metrics::compute_metrics(&s);
    Json(metrics.virality)
}

async fn get_influence(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;
    let metrics = crate::metrics::compute_metrics(&s);
    Json(metrics.influence)
}

async fn get_cascades(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;
    let cascade_stats = s.world.cascade_stats();
    let cascades: Vec<serde_json::Value> = cascade_stats.iter()
        .filter_map(|(root_id, size, depth)| {
            let post = s.world.posts.get(root_id)?;
            Some(serde_json::json!({
                "root_id": root_id.to_string(),
                "root_author": post.author_name,
                "root_content": post.content.chars().take(120).collect::<String>(),
                "size": size,
                "max_depth": depth,
                "engagement": post.engagement_score(),
            }))
        })
        .collect();
    Json(cascades)
}

async fn get_community_metrics(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;
    let metrics = crate::metrics::compute_metrics(&s);
    Json(metrics.community)
}

async fn get_contagion(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;
    let metrics = crate::metrics::compute_metrics(&s);
    Json(metrics.contagion)
}

async fn get_cognitive_metrics(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;
    let metrics = crate::metrics::compute_metrics(&s);
    Json(metrics.cognitive)
}

/// Compare current run metrics with a saved run.
async fn compare_runs(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;
    let current_metrics = crate::metrics::compute_metrics(&s);

    // Try to load saved state for comparison
    let save_path = s.config.output.output_dir.join("state.json");
    if !save_path.exists() {
        return Json(serde_json::json!({
            "error": "No saved state to compare. Save a run first with /api/simulation/save.",
            "current": current_metrics,
        })).into_response();
    }

    match crate::engine::SimulationState::load_from_file(&save_path) {
        Ok(saved_state) => {
            let saved_metrics = crate::metrics::compute_metrics(&saved_state);
            Json(serde_json::json!({
                "current": current_metrics,
                "saved": saved_metrics,
                "delta": {
                    "polarization_delta": current_metrics.polarization.polarization_index - saved_metrics.polarization.polarization_index,
                    "viral_count_delta": current_metrics.virality.viral_post_count as i32 - saved_metrics.virality.viral_post_count as i32,
                    "engagement_gini_delta": current_metrics.influence.engagement_gini - saved_metrics.influence.engagement_gini,
                    "echo_chamber_delta": current_metrics.community.echo_chamber_score - saved_metrics.community.echo_chamber_score,
                    "contested_delta": current_metrics.content.contested_count as i32 - saved_metrics.content.contested_count as i32,
                }
            })).into_response()
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": format!("Failed to load saved state: {e}"),
                "current": current_metrics,
            }))).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Validation & Export
// ---------------------------------------------------------------------------

async fn validate_state_endpoint(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;
    let issues = crate::engine::validate_state(&s);
    Json(serde_json::json!({
        "valid": issues.is_empty(),
        "issue_count": issues.len(),
        "issues": issues,
    }))
}

async fn export_json(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;
    let metrics = crate::metrics::compute_metrics(&s);

    // Build a comprehensive export
    let export = serde_json::json!({
        "metadata": {
            "scenario": s.config.simulation.scenario_prompt,
            "total_rounds": s.world.current_round,
            "total_agents": s.agents.len(),
            "total_posts": s.world.posts.len(),
            "total_actions": s.total_actions,
            "exported_at": chrono::Utc::now().to_rfc3339(),
        },
        "metrics": metrics,
        "agents": s.agents.values().map(|a| serde_json::json!({
            "id": a.id,
            "username": a.username,
            "name": a.name,
            "tier": a.tier.to_string(),
            "stance": a.stance.to_string(),
            "archetype": a.archetype.to_string(),
            "sentiment": s.agent_states.get(&a.id).map(|st| st.current_sentiment).unwrap_or(0.0),
            "fatigue": s.agent_states.get(&a.id).map(|st| st.cognitive.fatigue).unwrap_or(0.0),
            "post_count": s.agent_states.get(&a.id).map(|st| st.post_ids.len()).unwrap_or(0),
            "follower_count": s.world.social_graph.follower_count(&a.id),
            "beliefs": s.agent_states.get(&a.id).map(|st| &st.beliefs),
        })).collect::<Vec<_>>(),
        "round_summaries": s.world.round_summaries,
        "syntheses": s.syntheses,
    });

    Json(export)
}

async fn export_metrics_json(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.sim_state.read().await;
    let metrics = crate::metrics::compute_metrics(&s);
    Json(serde_json::json!({
        "scenario": s.config.simulation.scenario_prompt,
        "rounds": s.world.current_round,
        "agents": s.agents.len(),
        "metrics": metrics,
    }))
}

// ---------------------------------------------------------------------------
// WebSocket
// ---------------------------------------------------------------------------

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: AppState) {
    // Subscribe to the persistent ws_tx (survives sim restarts)
    let mut rx = state.ws_tx.subscribe();

    // Send current status on connect
    {
        let s = state.sim_state.read().await;
        let snapshot = s.snapshot();
        if let Ok(json) = serde_json::to_string(&snapshot) {
            let _ = socket
                .send(Message::Text(json.into()))
                .await;
        }
    }

    // Stream events to client
    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(event) => {
                        if let Ok(json) = serde_json::to_string(&event) {
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("WebSocket client lagged by {n} messages");
                    }
                    Err(_) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}
