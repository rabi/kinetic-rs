// SPDX-License-Identifier: MIT

use axum::{
    extract::Path,
    response::sse::{Event, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::Stream;
use serde::Deserialize;
use serde_json::{json, Value};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::kinetic::tools::{github, jira, search};
use crate::kinetic::workflow::builder::Builder;
use crate::kinetic::workflow::registry::ToolRegistry;

pub async fn serve(port: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let app = Router::new()
        .route("/api/health", get(health_check))
        .route("/api/workflows", get(list_workflows))
        .route("/api/workflows/{id}", get(get_workflow))
        .route("/api/agents", get(list_agents))
        .route("/api/agents/{id}", get(get_agent))
        .route("/api/executions", post(create_execution))
        .route("/api/executions/stream", post(stream_execution))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    log::info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn list_workflows() -> Json<Value> {
    let mut workflows = Vec::new();
    if let Ok(mut entries) = fs::read_dir("examples").await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
            {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    workflows.push(json!({
                        "id": stem,
                        "name": stem,
                        "file": path.to_string_lossy()
                    }));
                }
            }
        }
    }
    Json(json!(workflows))
}

async fn get_workflow(Path(id): Path<String>) -> Json<Value> {
    let path = PathBuf::from("examples").join(format!("{}.yaml", id));
    if !path.exists() {
        return Json(json!({"error": "Workflow not found"}));
    }

    match fs::read_to_string(&path).await {
        Ok(content) => match serde_yaml::from_str::<Value>(&content) {
            Ok(yaml) => Json(yaml),
            Err(e) => Json(json!({"error": format!("Invalid YAML: {}", e)})),
        },
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn list_agents() -> Json<Value> {
    let mut agents = Vec::new();
    if let Ok(mut entries) = fs::read_dir("agents").await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
            {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    agents.push(json!({
                        "id": stem,
                        "name": stem,
                         "file": path.to_string_lossy()
                    }));
                }
            }
        }
    }
    Json(json!(agents))
}

async fn get_agent(Path(id): Path<String>) -> Json<Value> {
    let path = PathBuf::from("agents").join(format!("{}.yaml", id));
    if !path.exists() {
        return Json(json!({"error": "Agent not found"}));
    }

    match fs::read_to_string(&path).await {
        Ok(content) => match serde_yaml::from_str::<Value>(&content) {
            Ok(yaml) => Json(yaml),
            Err(e) => Json(json!({"error": format!("Invalid YAML: {}", e)})),
        },
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

#[derive(Deserialize)]
struct ExecutionRequest {
    workflow_id: String,
    input: String,
}

// Register tools Helper
async fn register_tools(registry: &ToolRegistry) {
    if let Ok(search_tool) = search::BraveSearchTool::new() {
        registry.register(Arc::new(search_tool)).await;
    }
    if let Ok(github_tools) = github::create_tools() {
        for tool in github_tools {
            registry.register(tool).await;
        }
    }
    if let Ok(jira_tools) = jira::create_tools() {
        for tool in jira_tools {
            registry.register(tool).await;
        }
    }
}

async fn create_execution(Json(payload): Json<ExecutionRequest>) -> Json<Value> {
    let path = PathBuf::from("examples").join(format!("{}.yaml", payload.workflow_id));
    let workflow_path = if path.exists() {
        path
    } else {
        PathBuf::from("agents").join(format!("{}.yaml", payload.workflow_id))
    };

    if !workflow_path.exists() {
        return Json(json!({"error": "Workflow/Agent not found"}));
    }

    let registry = ToolRegistry::new();
    register_tools(&registry).await;

    let mcp_manager = Arc::new(crate::kinetic::mcp::manager::McpServiceManager::new());
    let builder = Builder::new(registry, mcp_manager);

    match builder.build_agent(workflow_path.to_str().unwrap()).await {
        Ok(agent) => match agent.run(payload.input).await {
            Ok(response) => Json(json!({ "status": "completed", "output": response })),
            Err(e) => Json(json!({ "error": format!("Execution failed: {}", e) })),
        },
        Err(e) => Json(json!({"error": format!("Failed to build agent: {}", e)})),
    }
}

async fn stream_execution(
    Json(payload): Json<ExecutionRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::channel(100);

    tokio::spawn(async move {
        log::info!(
            "Starting streaming execution for workflow/agent: {}",
            payload.workflow_id
        );

        // Determine path Logic (Copied from create_execution)
        let path = PathBuf::from("examples").join(format!("{}.yaml", payload.workflow_id));
        let workflow_path = if path.exists() {
            path
        } else {
            PathBuf::from("agents").join(format!("{}.yaml", payload.workflow_id))
        };

        if !workflow_path.exists() {
            log::warn!("Workflow/agent not found: {:?}", workflow_path);
            let _ = tx
                .send(crate::adk::agent::AgentEvent::Error(
                    "Workflow/Agent not found".into(),
                ))
                .await;
            return;
        }

        let registry = ToolRegistry::new();
        register_tools(&registry).await;

        let mcp_manager = Arc::new(crate::kinetic::mcp::manager::McpServiceManager::new());
        let builder = Builder::new(registry, mcp_manager);

        log::info!("Building agent from: {:?}", workflow_path);
        match builder.build_agent(workflow_path.to_str().unwrap()).await {
            Ok(agent) => {
                log::info!("Agent built successfully, starting run_stream");
                if let Err(e) = agent.run_stream(payload.input, tx.clone()).await {
                    log::error!("Agent execution failed: {}", e);
                    let _ = tx
                        .send(crate::adk::agent::AgentEvent::Error(format!(
                            "Execution Error: {}",
                            e
                        )))
                        .await;
                }
                log::info!("Agent execution finished");
            }
            Err(e) => {
                log::error!("Failed to build agent: {}", e);
                let _ = tx
                    .send(crate::adk::agent::AgentEvent::Error(format!(
                        "Build failed: {}",
                        e
                    )))
                    .await;
            }
        }
    });

    let stream =
        ReceiverStream::new(rx).map(|event| Ok(Event::default().json_data(event).unwrap()));

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new().interval(std::time::Duration::from_secs(1)),
    )
}
