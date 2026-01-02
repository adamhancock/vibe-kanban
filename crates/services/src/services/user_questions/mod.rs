pub mod executor_questions;

use std::{collections::HashMap, sync::Arc, time::Duration as StdDuration};

use dashmap::DashMap;
use db::models::{
    execution_process::ExecutionProcess,
    task::{Task, TaskStatus},
};
use executors::{
    approvals::ToolCallMetadata,
    logs::{
        NormalizedEntry, NormalizedEntryType, ToolStatus,
        utils::patch::{ConversationPatch, extract_normalized_entry_from_patch},
    },
};
use futures::future::{BoxFuture, FutureExt, Shared};
use sqlx::{Error as SqlxError, SqlitePool};
use thiserror::Error;
use tokio::sync::{RwLock, oneshot};
use utils::{
    log_msg::LogMsg,
    msg_store::MsgStore,
    user_questions::{UserQuestion, UserQuestionRequest, UserQuestionResponse},
};
use uuid::Uuid;

#[derive(Debug)]
struct PendingQuestion {
    entry_index: usize,
    entry: NormalizedEntry,
    execution_process_id: Uuid,
    #[allow(dead_code)]
    questions: Vec<UserQuestion>,
    response_tx: oneshot::Sender<UserQuestionResponse>,
}

type QuestionWaiter = Shared<BoxFuture<'static, Option<UserQuestionResponse>>>;

#[derive(Clone)]
pub struct UserQuestions {
    pending: Arc<DashMap<String, PendingQuestion>>,
    completed: Arc<DashMap<String, UserQuestionResponse>>,
    msg_stores: Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>>,
}

#[derive(Debug, Error)]
pub enum QuestionError {
    #[error("question request not found")]
    NotFound,
    #[error("question request already completed")]
    AlreadyCompleted,
    #[error("no executor session found for session_id: {0}")]
    NoExecutorSession(String),
    #[error("corresponding tool use entry not found for question request")]
    NoToolUseEntry,
    #[error(transparent)]
    Custom(#[from] anyhow::Error),
    #[error(transparent)]
    Sqlx(#[from] SqlxError),
}

impl UserQuestions {
    pub fn new(msg_stores: Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>>) -> Self {
        Self {
            pending: Arc::new(DashMap::new()),
            completed: Arc::new(DashMap::new()),
            msg_stores,
        }
    }

    pub async fn create_with_waiter(
        &self,
        request: UserQuestionRequest,
    ) -> Result<(UserQuestionRequest, QuestionWaiter), QuestionError> {
        let (tx, rx) = oneshot::channel();
        let waiter: QuestionWaiter = rx.map(|result| result.ok()).boxed().shared();
        let req_id = request.id.clone();

        if let Some(store) = self.msg_store_by_id(&request.execution_process_id).await {
            // Find the matching tool use entry by tool call id
            let matching_tool = find_matching_tool_use(store.clone(), &request.tool_call_id);

            if let Some((idx, matching_tool)) = matching_tool {
                let question_entry = matching_tool
                    .with_tool_status(ToolStatus::PendingQuestion {
                        question_id: req_id.clone(),
                        requested_at: request.created_at,
                        timeout_at: request.timeout_at,
                        questions: request.questions.clone(),
                    })
                    .ok_or(QuestionError::NoToolUseEntry)?;
                store.push_patch(ConversationPatch::replace(idx, question_entry));

                self.pending.insert(
                    req_id.clone(),
                    PendingQuestion {
                        entry_index: idx,
                        entry: matching_tool,
                        execution_process_id: request.execution_process_id,
                        questions: request.questions.clone(),
                        response_tx: tx,
                    },
                );
                tracing::debug!(
                    "Created question {} with {} questions at entry index {}",
                    req_id,
                    request.questions.len(),
                    idx
                );
            } else {
                tracing::warn!(
                    "No matching tool use entry found for question request: execution_process_id={}",
                    request.execution_process_id
                );
            }
        } else {
            tracing::warn!(
                "No msg_store found for execution_process_id: {}",
                request.execution_process_id
            );
        }

        self.spawn_timeout_watcher(req_id.clone(), request.timeout_at, waiter.clone());
        Ok((request, waiter))
    }

    #[tracing::instrument(skip(self, id, response))]
    pub async fn respond(
        &self,
        pool: &SqlitePool,
        id: &str,
        response: UserQuestionResponse,
    ) -> Result<UserQuestionResponse, QuestionError> {
        if let Some((_, p)) = self.pending.remove(id) {
            self.completed.insert(id.to_string(), response.clone());
            let _ = p.response_tx.send(response.clone());

            if let Some(store) = self.msg_store_by_id(&p.execution_process_id).await {
                // Mark the tool as successful after question is answered
                let updated_entry = p
                    .entry
                    .with_tool_status(ToolStatus::Success)
                    .ok_or(QuestionError::NoToolUseEntry)?;

                store.push_patch(ConversationPatch::replace(p.entry_index, updated_entry));
            } else {
                tracing::warn!(
                    "No msg_store found for execution_process_id: {}",
                    p.execution_process_id
                );
            }

            // Move task back to InProgress if in InReview
            if let Ok(ctx) = ExecutionProcess::load_context(pool, p.execution_process_id).await
                && ctx.task.status == TaskStatus::InReview
                && let Err(e) = Task::update_status(pool, ctx.task.id, TaskStatus::InProgress).await
            {
                tracing::warn!(
                    "Failed to update task status to InProgress after question response: {}",
                    e
                );
            }

            Ok(response)
        } else if self.completed.contains_key(id) {
            Err(QuestionError::AlreadyCompleted)
        } else {
            Err(QuestionError::NotFound)
        }
    }

    #[tracing::instrument(skip(self, id, timeout_at, waiter))]
    fn spawn_timeout_watcher(
        &self,
        id: String,
        timeout_at: chrono::DateTime<chrono::Utc>,
        waiter: QuestionWaiter,
    ) {
        let pending = self.pending.clone();
        let msg_stores = self.msg_stores.clone();

        let now = chrono::Utc::now();
        let to_wait = (timeout_at - now)
            .to_std()
            .unwrap_or_else(|_| StdDuration::from_secs(0));
        let deadline = tokio::time::Instant::now() + to_wait;

        tokio::spawn(async move {
            let result = tokio::select! {
                biased;

                resolved = waiter.clone() => resolved,
                _ = tokio::time::sleep_until(deadline) => None,
            };

            let is_timeout = result.is_none();

            if is_timeout && let Some((_, pending_question)) = pending.remove(&id) {
                let store = {
                    let map = msg_stores.read().await;
                    map.get(&pending_question.execution_process_id).cloned()
                };

                if let Some(store) = store {
                    if let Some(updated_entry) = pending_question
                        .entry
                        .with_tool_status(ToolStatus::TimedOut)
                    {
                        store.push_patch(ConversationPatch::replace(
                            pending_question.entry_index,
                            updated_entry,
                        ));
                    } else {
                        tracing::warn!(
                            "Timed out question '{}' but couldn't update tool status (no tool-use entry).",
                            id
                        );
                    }
                } else {
                    tracing::warn!(
                        "No msg_store found for execution_process_id: {}",
                        pending_question.execution_process_id
                    );
                }
            }
        });
    }

    async fn msg_store_by_id(&self, execution_process_id: &Uuid) -> Option<Arc<MsgStore>> {
        let map = self.msg_stores.read().await;
        map.get(execution_process_id).cloned()
    }
}

/// Find a matching tool use entry that hasn't been assigned to a question yet
/// Matches by tool call id from tool metadata
fn find_matching_tool_use(
    store: Arc<MsgStore>,
    tool_call_id: &str,
) -> Option<(usize, NormalizedEntry)> {
    let history = store.get_history();

    for msg in history.iter().rev() {
        if let LogMsg::JsonPatch(patch) = msg
            && let Some((idx, entry)) = extract_normalized_entry_from_patch(patch)
            && let NormalizedEntryType::ToolUse { status, .. } = &entry.entry_type
        {
            // Only match tools that are in Created state
            if !matches!(status, ToolStatus::Created) {
                continue;
            }

            // Match by tool call id from metadata
            if let Some(metadata) = &entry.metadata
                && let Ok(ToolCallMetadata {
                    tool_call_id: entry_call_id,
                    ..
                }) = serde_json::from_value::<ToolCallMetadata>(metadata.clone())
                && entry_call_id == tool_call_id
            {
                tracing::debug!(
                    "Matched tool use entry at index {idx} for tool call id '{tool_call_id}'"
                );
                return Some((idx, entry));
            }
        }
    }

    None
}
