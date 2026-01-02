use std::sync::Arc;

use workspace_utils::approvals::ApprovalStatus;
use workspace_utils::user_questions::UserQuestion;

use super::types::PermissionMode;
use crate::{
    approvals::{ExecutorApprovalError, ExecutorApprovalService, ExecutorQuestionService},
    executors::{
        ExecutorError,
        claude::{
            ClaudeJson,
            types::{
                PermissionResult, PermissionUpdate, PermissionUpdateDestination,
                PermissionUpdateType,
            },
        },
        codex::client::LogWriter,
    },
};

const EXIT_PLAN_MODE_NAME: &str = "ExitPlanMode";
const ASK_USER_QUESTION_NAME: &str = "AskUserQuestion";
pub const AUTO_APPROVE_CALLBACK_ID: &str = "AUTO_APPROVE_CALLBACK_ID";

/// Claude Agent client with control protocol support
pub struct ClaudeAgentClient {
    log_writer: LogWriter,
    approvals: Option<Arc<dyn ExecutorApprovalService>>,
    questions: Option<Arc<dyn ExecutorQuestionService>>,
    auto_approve: bool, // true when approvals is None
}

impl ClaudeAgentClient {
    /// Create a new client with optional approval and question services
    pub fn new(
        log_writer: LogWriter,
        approvals: Option<Arc<dyn ExecutorApprovalService>>,
        questions: Option<Arc<dyn ExecutorQuestionService>>,
    ) -> Arc<Self> {
        let auto_approve = approvals.is_none();
        Arc::new(Self {
            log_writer,
            approvals,
            questions,
            auto_approve,
        })
    }

    async fn handle_approval(
        &self,
        tool_use_id: String,
        tool_name: String,
        tool_input: serde_json::Value,
    ) -> Result<PermissionResult, ExecutorError> {
        // Use approval service to request tool approval
        let approval_service = self
            .approvals
            .as_ref()
            .ok_or(ExecutorApprovalError::ServiceUnavailable)?;
        let status = approval_service
            .request_tool_approval(&tool_name, tool_input.clone(), &tool_use_id)
            .await;
        match status {
            Ok(status) => {
                // Log the approval response so we it appears in the executor logs
                self.log_writer
                    .log_raw(&serde_json::to_string(&ClaudeJson::ApprovalResponse {
                        call_id: tool_use_id.clone(),
                        tool_name: tool_name.clone(),
                        approval_status: status.clone(),
                    })?)
                    .await?;
                match status {
                    ApprovalStatus::Approved => {
                        if tool_name == EXIT_PLAN_MODE_NAME {
                            Ok(PermissionResult::Allow {
                                updated_input: tool_input,
                                updated_permissions: Some(vec![PermissionUpdate {
                                    update_type: PermissionUpdateType::SetMode,
                                    mode: Some(PermissionMode::BypassPermissions),
                                    destination: PermissionUpdateDestination::Session,
                                }]),
                            })
                        } else {
                            Ok(PermissionResult::Allow {
                                updated_input: tool_input,
                                updated_permissions: None,
                            })
                        }
                    }
                    ApprovalStatus::Denied { reason } => {
                        let message = reason.unwrap_or("Denied by user".to_string());
                        Ok(PermissionResult::Deny {
                            message,
                            interrupt: Some(false),
                        })
                    }
                    ApprovalStatus::TimedOut => Ok(PermissionResult::Deny {
                        message: "Approval request timed out".to_string(),
                        interrupt: Some(false),
                    }),
                    ApprovalStatus::Pending => Ok(PermissionResult::Deny {
                        message: "Approval still pending (unexpected)".to_string(),
                        interrupt: Some(false),
                    }),
                }
            }
            Err(e) => {
                tracing::error!("Tool approval request failed: {e}");
                Ok(PermissionResult::Deny {
                    message: "Tool approval request failed".to_string(),
                    interrupt: Some(false),
                })
            }
        }
    }

    async fn handle_user_question(
        &self,
        tool_use_id: String,
        questions: Vec<UserQuestion>,
    ) -> Result<PermissionResult, ExecutorError> {
        let question_service = self.questions.as_ref().ok_or(
            crate::approvals::ExecutorQuestionError::ServiceUnavailable
        )?;

        match question_service
            .request_user_question(&tool_use_id, questions.clone())
            .await
        {
            Ok(response) => {
                // Format answers in the way Claude Code expects
                // The answers are keyed by question header (or index if no header)
                let mut answers_map = serde_json::Map::new();
                for answer in &response.answers {
                    if let Some(question) = questions.get(answer.question_index) {
                        let key = question.header.clone().unwrap_or_else(|| {
                            format!("question_{}", answer.question_index)
                        });

                        // If there's custom text (Other option), use that
                        if let Some(custom) = &answer.custom_text {
                            answers_map.insert(key, serde_json::Value::String(custom.clone()));
                        } else if question.multi_select {
                            // For multi-select, return array of selected labels
                            let selected_labels: Vec<serde_json::Value> = answer
                                .selected_options
                                .iter()
                                .filter_map(|&idx| {
                                    question.options.get(idx).map(|opt| {
                                        serde_json::Value::String(opt.label.clone())
                                    })
                                })
                                .collect();
                            answers_map.insert(key, serde_json::Value::Array(selected_labels));
                        } else {
                            // For single-select, return the selected label
                            if let Some(&idx) = answer.selected_options.first() {
                                if let Some(opt) = question.options.get(idx) {
                                    answers_map.insert(
                                        key,
                                        serde_json::Value::String(opt.label.clone()),
                                    );
                                }
                            }
                        }
                    }
                }

                // Return allow with the answers included
                // Claude Code should receive these as the tool result
                let result_input = serde_json::json!({
                    "questions": questions,
                    "answers": answers_map
                });

                Ok(PermissionResult::Allow {
                    updated_input: result_input,
                    updated_permissions: None,
                })
            }
            Err(e) => {
                tracing::error!("User question request failed: {e}");
                Ok(PermissionResult::Deny {
                    message: format!("User question request failed: {e}"),
                    interrupt: Some(false),
                })
            }
        }
    }

    pub async fn on_can_use_tool(
        &self,
        tool_name: String,
        input: serde_json::Value,
        _permission_suggestions: Option<Vec<PermissionUpdate>>,
        tool_use_id: Option<String>,
    ) -> Result<PermissionResult, ExecutorError> {
        if self.auto_approve {
            Ok(PermissionResult::Allow {
                updated_input: input,
                updated_permissions: None,
            })
        } else if let Some(latest_tool_use_id) = tool_use_id {
            // Handle AskUserQuestion specially
            if tool_name == ASK_USER_QUESTION_NAME {
                // Parse questions from input
                if let Ok(questions) = serde_json::from_value::<Vec<UserQuestion>>(
                    input.get("questions").cloned().unwrap_or(serde_json::Value::Array(vec![])),
                ) {
                    return self
                        .handle_user_question(latest_tool_use_id, questions)
                        .await;
                } else {
                    tracing::warn!("Failed to parse AskUserQuestion input, falling back to approval");
                }
            }

            self.handle_approval(latest_tool_use_id, tool_name, input)
                .await
        } else {
            // Auto approve tools with no matching tool_use_id
            // tool_use_id is undocumented so this may not be possible
            tracing::warn!(
                "No tool_use_id available for tool '{}', cannot request approval",
                tool_name
            );
            Ok(PermissionResult::Allow {
                updated_input: input,
                updated_permissions: None,
            })
        }
    }

    pub async fn on_hook_callback(
        &self,
        callback_id: String,
        _input: serde_json::Value,
        _tool_use_id: Option<String>,
    ) -> Result<serde_json::Value, ExecutorError> {
        if self.auto_approve {
            Ok(serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "allow",
                    "permissionDecisionReason": "Auto-approved by SDK"
                }
            }))
        } else {
            match callback_id.as_str() {
                AUTO_APPROVE_CALLBACK_ID => Ok(serde_json::json!({
                    "hookSpecificOutput": {
                        "hookEventName": "PreToolUse",
                        "permissionDecision": "allow",
                        "permissionDecisionReason": "Approved by SDK"
                    }
                })),
                _ => {
                    // Hook callbacks is only used to forward approval requests to can_use_tool.
                    // This works because `ask` decision in hook callback triggers a can_use_tool request
                    // https://docs.claude.com/en/api/agent-sdk/permissions#permission-flow-diagram
                    Ok(serde_json::json!({
                        "hookSpecificOutput": {
                            "hookEventName": "PreToolUse",
                            "permissionDecision": "ask",
                            "permissionDecisionReason": "Forwarding to canusetool service"
                        }
                    }))
                }
            }
        }
    }

    pub async fn on_non_control(&self, line: &str) -> Result<(), ExecutorError> {
        // Forward all non-control messages to stdout
        self.log_writer.log_raw(line).await
    }
}
