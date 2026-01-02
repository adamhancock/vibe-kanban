use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

const REDIS_KEY: &str = "workstream:notion:tasks";

/// Notion task structure from Redis (workstream-daemon format)
#[derive(Debug, Clone, Deserialize)]
pub struct NotionTask {
    pub id: String,
    #[serde(rename = "taskId")]
    pub task_id: String,
    pub title: String,
    #[serde(rename = "branchName")]
    pub branch_name: String,
    pub status: String,
    #[serde(rename = "statusGroup")]
    pub status_group: String,
    #[serde(rename = "type")]
    pub task_type: Option<String>,
    pub url: String,
    #[serde(rename = "contentMarkdown")]
    pub content_markdown: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum RedisClientError {
    #[error("Redis URL not configured. Set REDIS_URL environment variable.")]
    NotConfigured,
    #[error("Redis connection error: {0}")]
    Connection(#[from] redis::RedisError),
    #[error("Failed to parse tasks: {0}")]
    Parse(#[from] serde_json::Error),
}

#[derive(Clone)]
pub struct RedisClient {
    connection: Arc<RwLock<Option<ConnectionManager>>>,
    url: Option<String>,
}

const DEFAULT_REDIS_URL: &str = "redis://localhost:6379";

impl RedisClient {
    pub fn new() -> Self {
        let url = std::env::var("REDIS_URL")
            .ok()
            .unwrap_or_else(|| DEFAULT_REDIS_URL.to_string());
        tracing::info!("Redis client initialized with URL: {}", url);
        Self {
            connection: Arc::new(RwLock::new(None)),
            url: Some(url),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.url.is_some()
    }

    async fn get_connection(&self) -> Result<ConnectionManager, RedisClientError> {
        let url = self.url.as_ref().ok_or(RedisClientError::NotConfigured)?;

        // Check if we have an existing connection
        {
            let guard = self.connection.read().await;
            if let Some(conn) = guard.as_ref() {
                return Ok(conn.clone());
            }
        }

        // Create new connection
        let client = redis::Client::open(url.as_str())?;
        let conn = ConnectionManager::new(client).await?;

        // Store for reuse
        {
            let mut guard = self.connection.write().await;
            *guard = Some(conn.clone());
        }

        Ok(conn)
    }

    pub async fn get_notion_tasks(&self) -> Result<Vec<NotionTask>, RedisClientError> {
        let mut conn = self.get_connection().await?;

        let data: Option<String> = conn.get(REDIS_KEY).await?;

        match data {
            Some(json) => {
                let tasks: Vec<NotionTask> = serde_json::from_str(&json)?;
                Ok(tasks)
            }
            None => Ok(vec![]),
        }
    }
}

impl Default for RedisClient {
    fn default() -> Self {
        Self::new()
    }
}
