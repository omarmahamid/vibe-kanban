use anyhow::{Context, Result};
use db::models::task::{CreateTask, Task, TaskStatus};
use reqwest::header;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use url::Url;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct YouTrackAuthToken(pub String);

#[derive(Debug, Clone, Serialize)]
pub struct SyncSummary {
    pub open_issues_total: usize,
    pub created: usize,
    pub skipped_existing: usize,
    pub dry_run: bool,
    pub created_titles: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct YouTrackIssue {
    #[serde(rename = "idReadable")]
    id_readable: String,
    summary: String,
    description: Option<String>,
    #[serde(rename = "customFields", default)]
    custom_fields: Vec<YouTrackCustomField>,
}

#[derive(Debug, Deserialize)]
struct YouTrackCustomField {
    name: String,
    value: Option<serde_json::Value>,
}

fn youtrack_state_value(issue: &YouTrackIssue, state_field: &str) -> Option<String> {
    let field = issue
        .custom_fields
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case(state_field))?;
    let value = field.value.as_ref()?;
    if let Some(name) = value.get("name").and_then(|v| v.as_str()) {
        return Some(name.to_string());
    }
    value.as_str().map(|s| s.to_string())
}

fn is_open_issue(issue: &YouTrackIssue, state_field: &str, open_value: &str) -> bool {
    youtrack_state_value(issue, state_field)
        .is_some_and(|v| v.eq_ignore_ascii_case(open_value))
}

fn issue_title_prefix(issue_id_readable: &str) -> String {
    format!("[{issue_id_readable}] ")
}

fn issue_url(youtrack_base: &Url, issue_id_readable: &str) -> Result<Url> {
    youtrack_base
        .join(&format!("issue/{issue_id_readable}"))
        .with_context(|| format!("failed to build issue URL for {issue_id_readable}"))
}

fn normalize_base_url(mut base: Url) -> Result<Url> {
    if !base.as_str().ends_with('/') {
        base = Url::parse(&format!("{}/", base.as_str())).context("invalid base URL")?;
    }
    Ok(base)
}

pub fn parse_board_url(board_url: &str) -> Result<(Url, String, String)> {
    let url = Url::parse(board_url).context("invalid YouTrack board URL")?;
    let segments: Vec<_> = url
        .path_segments()
        .map(|s| s.collect::<Vec<_>>())
        .unwrap_or_default();

    let agiles_index = segments
        .iter()
        .position(|s| s.eq_ignore_ascii_case("agiles"))
        .context("board URL must contain '/agiles/{agileId}/{sprintId}'")?;

    let agile_id = segments
        .get(agiles_index + 1)
        .context("missing agile id segment")?
        .to_string();
    let sprint_id = segments
        .get(agiles_index + 2)
        .context("missing sprint id segment")?
        .to_string();

    let mut base = url.clone();
    let prefix_segments = &segments[..agiles_index];
    let mut prefix_path = String::from("/");
    if !prefix_segments.is_empty() {
        prefix_path.push_str(&prefix_segments.join("/"));
        prefix_path.push('/');
    }
    base.set_path(&prefix_path);
    base.set_query(None);
    base.set_fragment(None);

    Ok((normalize_base_url(base)?, agile_id, sprint_id))
}

async fn fetch_sprint_issues(
    http: &reqwest::Client,
    youtrack_base: &Url,
    agile_id: &str,
    sprint_id: &str,
) -> Result<Vec<YouTrackIssue>> {
    let mut all: Vec<YouTrackIssue> = Vec::new();
    let mut skip = 0;
    let top = 100;

    loop {
        let url = youtrack_base
            .join(&format!("api/agiles/{agile_id}/sprints/{sprint_id}/issues"))
            .context("failed to build YouTrack sprint issues URL")?;

        let issues: Vec<YouTrackIssue> = http
            .get(url)
            .query(&[
                ("$skip", skip.to_string()),
                ("$top", top.to_string()),
                (
                    "fields",
                    "idReadable,summary,description,customFields(name,value(name))".to_string(),
                ),
            ])
            .send()
            .await
            .context("YouTrack request failed")?
            .error_for_status()
            .context("YouTrack returned an error status")?
            .json()
            .await
            .context("failed to decode YouTrack issues JSON")?;

        let batch_count = issues.len();
        all.extend(issues);

        if batch_count < top {
            break;
        }
        skip += top;
    }

    Ok(all)
}

fn http_client(token: YouTrackAuthToken) -> Result<reqwest::Client> {
    let mut auth_value = header::HeaderValue::from_str(&format!("Bearer {}", token.0))
        .context("invalid YouTrack token for Authorization header")?;
    auth_value.set_sensitive(true);

    let mut headers = header::HeaderMap::new();
    headers.insert(header::AUTHORIZATION, auth_value);

    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .context("failed to build HTTP client")
}

pub async fn sync_open_sprint_issues_to_todo(
    pool: &SqlitePool,
    project_id: Uuid,
    youtrack_base_url: Url,
    token: YouTrackAuthToken,
    agile_id: &str,
    sprint_id: &str,
    state_field: &str,
    open_value: &str,
    dry_run: bool,
) -> Result<SyncSummary> {
    let youtrack_base_url = normalize_base_url(youtrack_base_url)?;
    let http = http_client(token)?;

    let issues = fetch_sprint_issues(&http, &youtrack_base_url, agile_id, sprint_id).await?;
    let open_issues: Vec<_> = issues
        .into_iter()
        .filter(|i| is_open_issue(i, state_field, open_value))
        .collect();

    let mut created = 0usize;
    let mut skipped_existing = 0usize;
    let mut created_titles = Vec::new();

    for issue in open_issues.iter() {
        let prefix = issue_title_prefix(&issue.id_readable);
        if Task::find_by_project_id_and_title_prefix(pool, project_id, &prefix)
            .await?
            .is_some()
        {
            skipped_existing += 1;
            continue;
        }

        let url = issue_url(&youtrack_base_url, &issue.id_readable)?;
        let description = {
            let mut desc = String::new();
            desc.push_str(&format!("YouTrack: {url}\n"));
            if let Some(body) = issue.description.as_deref().filter(|s| !s.trim().is_empty()) {
                desc.push('\n');
                desc.push_str(body);
            }
            Some(desc)
        };

        let title = format!("{prefix}{}", issue.summary);
        let payload = CreateTask {
            project_id,
            title: title.clone(),
            description,
            status: Some(TaskStatus::Todo),
            parent_workspace_id: None,
            image_ids: None,
            shared_task_id: None,
        };

        if dry_run {
            created += 1;
            created_titles.push(title);
            continue;
        }

        let _task = Task::create(pool, &payload, Uuid::new_v4()).await?;
        created += 1;
        created_titles.push(title);
    }

    Ok(SyncSummary {
        open_issues_total: open_issues.len(),
        created,
        skipped_existing,
        dry_run,
        created_titles,
    })
}

