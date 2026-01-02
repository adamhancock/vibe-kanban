use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

pub const QUESTION_TIMEOUT_SECONDS: i64 = 3600; // 1 hour

/// A single question option with label and optional description
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct QuestionOption {
    pub label: String,
    #[serde(default)]
    #[ts(optional)]
    pub description: Option<String>,
}

/// A user question with options for selection
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct UserQuestion {
    pub question: String,
    #[serde(default)]
    #[ts(optional)]
    pub header: Option<String>,
    #[serde(default)]
    pub options: Vec<QuestionOption>,
    #[serde(default, rename = "multiSelect")]
    pub multi_select: bool,
}

/// Input format from Claude Code's AskUserQuestion tool
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AskUserQuestionInput {
    pub questions: Vec<UserQuestion>,
}

/// A user's answer to a single question
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct QuestionAnswer {
    pub question_index: usize,
    pub selected_options: Vec<usize>,
    #[serde(default)]
    #[ts(optional)]
    pub custom_text: Option<String>,
}

/// Request to create a pending user question
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CreateUserQuestionRequest {
    pub tool_call_id: String,
    pub questions: Vec<UserQuestion>,
}

/// Full user question request with metadata
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UserQuestionRequest {
    pub id: String,
    pub tool_call_id: String,
    pub questions: Vec<UserQuestion>,
    pub execution_process_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub timeout_at: DateTime<Utc>,
}

impl UserQuestionRequest {
    pub fn from_create(request: CreateUserQuestionRequest, execution_process_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            tool_call_id: request.tool_call_id,
            questions: request.questions,
            execution_process_id,
            created_at: now,
            timeout_at: now + Duration::seconds(QUESTION_TIMEOUT_SECONDS),
        }
    }
}

/// Status of a user question
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum UserQuestionStatus {
    Pending,
    Answered,
    TimedOut,
}

/// Response from the user answering questions
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UserQuestionResponse {
    pub execution_process_id: Uuid,
    pub answers: Vec<QuestionAnswer>,
}
