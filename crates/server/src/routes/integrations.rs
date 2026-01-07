use axum::{Json, Router, extract::State, response::Json as ResponseJson, routing::post};
use deployment::Deployment;
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};
use utils::response::ApiResponse;

use crate::integrations::youtrack_open::{
    YouTrackAuthToken, parse_board_url, sync_open_sprint_issues_to_todo,
};

#[derive(Debug, Deserialize)]
pub struct YouTrackOpenSyncRequest {
    pub project_id: Uuid,

    /// Full board URL like https://host/youtrack/agiles/{agileId}/{sprintId}?...
    pub board_url: Option<String>,

    /// Optional override when not using board_url
    pub youtrack_base_url: Option<String>,
    pub agile_id: Option<String>,
    pub sprint_id: Option<String>,

    pub youtrack_token: String,

    #[serde(default = "default_state_field")]
    pub state_field: String,

    #[serde(default = "default_open_value")]
    pub open_value: String,

    #[serde(default)]
    pub dry_run: bool,
}

fn default_state_field() -> String {
    "State".to_string()
}

fn default_open_value() -> String {
    "Open".to_string()
}

#[derive(Debug, Serialize)]
pub struct YouTrackOpenSyncResponse {
    pub open_issues_total: usize,
    pub created: usize,
    pub skipped_existing: usize,
    pub dry_run: bool,
    pub created_titles: Vec<String>,
}

pub async fn sync_youtrack_open(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<YouTrackOpenSyncRequest>,
) -> Result<ResponseJson<ApiResponse<YouTrackOpenSyncResponse>>, ApiError> {
    let (base_url, agile_id, sprint_id) = if let Some(board_url) = payload.board_url.as_deref() {
        parse_board_url(board_url).map_err(|e| ApiError::BadRequest(e.to_string()))?
    } else {
        let base = payload
            .youtrack_base_url
            .as_deref()
            .ok_or_else(|| ApiError::BadRequest("missing youtrack_base_url".to_string()))?;
        let agile_id = payload
            .agile_id
            .as_deref()
            .ok_or_else(|| ApiError::BadRequest("missing agile_id".to_string()))?;
        let sprint_id = payload
            .sprint_id
            .as_deref()
            .ok_or_else(|| ApiError::BadRequest("missing sprint_id".to_string()))?;
        (
            Url::parse(base).map_err(|e| ApiError::BadRequest(e.to_string()))?,
            agile_id.to_string(),
            sprint_id.to_string(),
        )
    };

    let summary = sync_open_sprint_issues_to_todo(
        &deployment.db().pool,
        payload.project_id,
        base_url,
        YouTrackAuthToken(payload.youtrack_token),
        &agile_id,
        &sprint_id,
        &payload.state_field,
        &payload.open_value,
        payload.dry_run,
    )
    .await
    .map_err(|e| ApiError::Upstream(e.to_string()))?;

    Ok(ResponseJson(ApiResponse::success(YouTrackOpenSyncResponse {
        open_issues_total: summary.open_issues_total,
        created: summary.created,
        skipped_existing: summary.skipped_existing,
        dry_run: summary.dry_run,
        created_titles: summary.created_titles,
    })))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new().route("/integrations/youtrack/open-sync", post(sync_youtrack_open))
}
