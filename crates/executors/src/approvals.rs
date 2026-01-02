use std::fmt;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use workspace_utils::approvals::ApprovalStatus;
use workspace_utils::user_questions::{UserQuestion, UserQuestionResponse};

/// Errors emitted by executor approval services.
#[derive(Debug, Error)]
pub enum ExecutorApprovalError {
    #[error("executor approval session not registered")]
    SessionNotRegistered,
    #[error("executor approval request failed: {0}")]
    RequestFailed(String),
    #[error("executor approval service unavailable")]
    ServiceUnavailable,
}

impl ExecutorApprovalError {
    pub fn request_failed<E: fmt::Display>(err: E) -> Self {
        Self::RequestFailed(err.to_string())
    }
}

/// Abstraction for executor approval backends.
#[async_trait]
pub trait ExecutorApprovalService: Send + Sync {
    /// Requests approval for a tool invocation and waits for the final decision.
    async fn request_tool_approval(
        &self,
        tool_name: &str,
        tool_input: Value,
        tool_call_id: &str,
    ) -> Result<ApprovalStatus, ExecutorApprovalError>;
}

#[derive(Debug, Default)]
pub struct NoopExecutorApprovalService;

#[async_trait]
impl ExecutorApprovalService for NoopExecutorApprovalService {
    async fn request_tool_approval(
        &self,
        _tool_name: &str,
        _tool_input: Value,
        _tool_call_id: &str,
    ) -> Result<ApprovalStatus, ExecutorApprovalError> {
        Ok(ApprovalStatus::Approved)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallMetadata {
    pub tool_call_id: String,
}

/// Errors emitted by executor question services.
#[derive(Debug, Error)]
pub enum ExecutorQuestionError {
    #[error("executor question session not registered")]
    SessionNotRegistered,
    #[error("executor question request failed: {0}")]
    RequestFailed(String),
    #[error("executor question service unavailable")]
    ServiceUnavailable,
    #[error("question timed out")]
    TimedOut,
}

impl ExecutorQuestionError {
    pub fn request_failed<E: fmt::Display>(err: E) -> Self {
        Self::RequestFailed(err.to_string())
    }
}

/// Abstraction for executor question backends.
#[async_trait]
pub trait ExecutorQuestionService: Send + Sync {
    /// Requests user to answer questions and waits for the response.
    async fn request_user_question(
        &self,
        tool_call_id: &str,
        questions: Vec<UserQuestion>,
    ) -> Result<UserQuestionResponse, ExecutorQuestionError>;
}
