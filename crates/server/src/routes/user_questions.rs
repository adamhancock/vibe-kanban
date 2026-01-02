use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
};
use deployment::Deployment;
use utils::user_questions::UserQuestionResponse;

use crate::DeploymentImpl;

pub async fn respond_to_question(
    State(deployment): State<DeploymentImpl>,
    Path(id): Path<String>,
    Json(response): Json<UserQuestionResponse>,
) -> Result<Json<UserQuestionResponse>, StatusCode> {
    let service = deployment.user_questions();

    match service.respond(&deployment.db().pool, &id, response).await {
        Ok(response) => {
            deployment
                .track_if_analytics_allowed(
                    "question_responded",
                    serde_json::json!({
                        "question_id": &id,
                        "answer_count": response.answers.len(),
                    }),
                )
                .await;

            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!("Failed to respond to question: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new().route("/questions/{id}/respond", post(respond_to_question))
}
