use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};
use tower_http::cors::{self, CorsLayer};
use tower_http::services::ServeDir;
use uuid::Uuid;

use crate::agent::Tier;
use crate::engine::{EngineControls, SharedState, WsEvent};
use crate::llm::PostSummary;
use crate::world::InjectedEvent;

// ---------------------------------------------------------------------------
// App state for Axum
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AppState {
    pub sim_state: SharedState,
    pub controls: Arc<EngineControls>,
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn create_router(app_state: AppState, web_dir: &str, cors_permissive: bool) -> Router {
    let cors = if cors_permissive {
        CorsLayer::permissive()
    } else {
        // Restrict to same-origin by default — only allow localhost origins
        CorsLayer::new()
            .allow_origin(cors::Any) // Static files are served from same origin; API is same-origin
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
        .route("/api/simulation/pause", post(pause_simulation))
        .route("/api/simulation/resume", post(resume_simulation))
        .route("/api/simulation/stop", post(stop_simulation))
        .route("/api/god-eye/inject", post(inject_event))
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

    // Sort: Tier1 first, then by follower count
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
        .map(|a| GraphNode {
            id: a.id.to_string(),
            label: format!("@{}", a.username),
            tier: a.tier.to_string(),
            size: match a.tier {
                Tier::Tier1 => 20.0,
                Tier::Tier2 => 12.0,
                Tier::Tier3 => 6.0,
            },
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
// Simulation control
// ---------------------------------------------------------------------------

async fn pause_simulation(State(state): State<AppState>) -> impl IntoResponse {
    let _ = state.controls.pause_tx.send(()).await;
    Json(serde_json::json!({"status": "paused"}))
}

async fn resume_simulation(State(state): State<AppState>) -> impl IntoResponse {
    let _ = state.controls.resume_tx.send(()).await;
    Json(serde_json::json!({"status": "resumed"}))
}

async fn stop_simulation(State(state): State<AppState>) -> impl IntoResponse {
    let _ = state.controls.stop_tx.send(()).await;
    Json(serde_json::json!({"status": "stopping"}))
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
    // Limit event content size to prevent abuse
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

    match state.controls.god_eye_tx.send(event.clone()).await {
        Ok(_) => Json(serde_json::json!({"status": "injected", "event_id": event.id})).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to inject event"})),
        )
            .into_response(),
    }
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
    let mut rx = state.controls.ws_tx.subscribe();

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
                    _ => {} // Ignore client messages for now
                }
            }
        }
    }
}
