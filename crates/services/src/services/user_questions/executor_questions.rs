use std::sync::Arc;

use async_trait::async_trait;
use db::{self, DBService};
use executors::approvals::{ExecutorQuestionError, ExecutorQuestionService};
use utils::user_questions::{
    CreateUserQuestionRequest, UserQuestion, UserQuestionRequest, UserQuestionResponse,
};
use uuid::Uuid;

use super::UserQuestions;
use crate::services::{approvals::ensure_task_in_review, notification::NotificationService};

pub struct ExecutorQuestionBridge {
    questions: UserQuestions,
    db: DBService,
    notification_service: NotificationService,
    execution_process_id: Uuid,
}

impl ExecutorQuestionBridge {
    pub fn new(
        questions: UserQuestions,
        db: DBService,
        notification_service: NotificationService,
        execution_process_id: Uuid,
    ) -> Arc<Self> {
        Arc::new(Self {
            questions,
            db,
            notification_service,
            execution_process_id,
        })
    }
}

#[async_trait]
impl ExecutorQuestionService for ExecutorQuestionBridge {
    async fn request_user_question(
        &self,
        tool_call_id: &str,
        questions: Vec<UserQuestion>,
    ) -> Result<UserQuestionResponse, ExecutorQuestionError> {
        ensure_task_in_review(&self.db.pool, self.execution_process_id).await;

        let request = UserQuestionRequest::from_create(
            CreateUserQuestionRequest {
                tool_call_id: tool_call_id.to_string(),
                questions: questions.clone(),
            },
            self.execution_process_id,
        );

        let (_, waiter) = self
            .questions
            .create_with_waiter(request)
            .await
            .map_err(|e| ExecutorQuestionError::request_failed(e.to_string()))?;

        // Play notification sound when question needs answering
        let question_count = questions.len();
        self.notification_service
            .notify(
                "Question from Agent",
                &format!(
                    "Agent is asking {} question{}",
                    question_count,
                    if question_count == 1 { "" } else { "s" }
                ),
            )
            .await;

        let response = waiter.clone().await;

        match response {
            Some(r) => Ok(r),
            None => Err(ExecutorQuestionError::TimedOut),
        }
    }
}
