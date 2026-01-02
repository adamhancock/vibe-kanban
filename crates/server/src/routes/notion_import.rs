use axum::{
    Router,
    extract::{Path, State},
    response::Json as ResponseJson,
    routing::{get, post},
};
use db::models::task::{CreateTask, Task, TaskStatus};
use deployment::Deployment;
use local_deployment::{NotionTask, RedisClientError};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

/// Preview item showing import status for each task
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct NotionImportPreviewItem {
    pub notion_id: String,
    pub task_id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub url: String,
    pub will_import: bool,
    pub skip_reason: Option<String>,
}

/// Preview response
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct NotionImportPreviewResponse {
    pub tasks: Vec<NotionImportPreviewItem>,
    pub total_count: usize,
    pub importable_count: usize,
    pub duplicate_count: usize,
}

/// Import request - which tasks to import
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct NotionImportRequest {
    pub notion_ids: Vec<String>,
}

/// Import result
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct NotionImportResponse {
    pub imported_count: usize,
    pub skipped_count: usize,
    pub errors: Vec<NotionImportError>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct NotionImportError {
    pub notion_id: String,
    pub title: String,
    pub error: String,
}

fn map_status_group(status_group: &str) -> TaskStatus {
    match status_group {
        "to_do" => TaskStatus::Todo,
        "in_progress" => TaskStatus::InProgress,
        "complete" => TaskStatus::Done,
        _ => TaskStatus::Todo,
    }
}

fn redis_error_to_api_error(err: RedisClientError) -> ApiError {
    match err {
        RedisClientError::NotConfigured => {
            ApiError::BadRequest("Redis not configured. Set REDIS_URL environment variable.".to_string())
        }
        RedisClientError::Connection(e) => {
            tracing::error!("Redis connection error: {}", e);
            ApiError::BadRequest(format!("Redis connection error: {}", e))
        }
        RedisClientError::Parse(e) => {
            tracing::error!("Redis parse error: {}", e);
            ApiError::BadRequest(format!("Failed to parse Notion tasks: {}", e))
        }
    }
}

pub async fn preview_notion_import(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<NotionImportPreviewResponse>>, ApiError> {
    let redis = deployment.redis_client();

    if !redis.is_configured() {
        return Err(ApiError::BadRequest(
            "Redis not configured. Set REDIS_URL environment variable.".to_string(),
        ));
    }

    // Fetch from Redis
    let notion_tasks = redis
        .get_notion_tasks()
        .await
        .map_err(redis_error_to_api_error)?;

    // Get existing task titles for duplicate detection
    let existing_tasks =
        Task::find_by_project_id_with_attempt_status(&deployment.db().pool, project_id).await?;

    let existing_titles: HashSet<String> = existing_tasks
        .iter()
        .map(|t| t.title.to_lowercase())
        .collect();

    // Build preview
    let mut preview_items = Vec::new();
    let mut duplicate_count = 0;

    for task in notion_tasks {
        let is_duplicate = existing_titles.contains(&task.title.to_lowercase());
        if is_duplicate {
            duplicate_count += 1;
        }

        preview_items.push(NotionImportPreviewItem {
            notion_id: task.id.clone(),
            task_id: task.task_id.clone(),
            title: task.title.clone(),
            description: task.content_markdown.clone(),
            status: map_status_group(&task.status_group),
            url: task.url.clone(),
            will_import: !is_duplicate,
            skip_reason: if is_duplicate {
                Some("Task with same title already exists".to_string())
            } else {
                None
            },
        });
    }

    let total_count = preview_items.len();
    let importable_count = total_count - duplicate_count;

    Ok(ResponseJson(ApiResponse::success(
        NotionImportPreviewResponse {
            tasks: preview_items,
            total_count,
            importable_count,
            duplicate_count,
        },
    )))
}

pub async fn execute_notion_import(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
    axum::Json(request): axum::Json<NotionImportRequest>,
) -> Result<ResponseJson<ApiResponse<NotionImportResponse>>, ApiError> {
    let redis = deployment.redis_client();

    if !redis.is_configured() {
        return Err(ApiError::BadRequest(
            "Redis not configured. Set REDIS_URL environment variable.".to_string(),
        ));
    }

    let notion_tasks = redis
        .get_notion_tasks()
        .await
        .map_err(redis_error_to_api_error)?;

    // Create a set of requested IDs for efficient lookup
    let requested_ids: HashSet<&str> = request.notion_ids.iter().map(|s| s.as_str()).collect();

    // Filter to requested IDs
    let tasks_to_import: Vec<&NotionTask> = notion_tasks
        .iter()
        .filter(|t| requested_ids.contains(t.id.as_str()))
        .collect();

    let mut imported_count = 0;
    let mut errors = Vec::new();

    for notion_task in tasks_to_import {
        let create_task = CreateTask {
            project_id,
            title: notion_task.title.clone(),
            description: notion_task.content_markdown.clone(),
            status: Some(map_status_group(&notion_task.status_group)),
            parent_workspace_id: None,
            image_ids: None,
            shared_task_id: None,
        };

        match Task::create(&deployment.db().pool, &create_task, Uuid::new_v4()).await {
            Ok(_) => imported_count += 1,
            Err(e) => errors.push(NotionImportError {
                notion_id: notion_task.id.clone(),
                title: notion_task.title.clone(),
                error: e.to_string(),
            }),
        }
    }

    let skipped_count = request.notion_ids.len() - imported_count - errors.len();

    Ok(ResponseJson(ApiResponse::success(NotionImportResponse {
        imported_count,
        skipped_count,
        errors,
    })))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route(
            "/projects/{project_id}/import/notion/preview",
            get(preview_notion_import),
        )
        .route(
            "/projects/{project_id}/import/notion",
            post(execute_notion_import),
        )
}
