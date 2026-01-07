use anyhow::{Context, Result};
use clap::Parser;
use deployment::Deployment;
use server::DeploymentImpl;
use uuid::Uuid;
use url::Url;

use server::integrations::youtrack_open::{YouTrackAuthToken, sync_open_sprint_issues_to_todo};

#[derive(Debug, Parser)]
#[command(about = "Sync Open YouTrack sprint issues into Vibe Kanban as Todo tasks")]
struct Args {
    /// Vibe Kanban project UUID to create tasks in
    #[arg(long, env = "VK_PROJECT_ID")]
    project_id: Uuid,

    /// YouTrack base URL, e.g. https://track.personetics.com/youtrack/
    #[arg(long, env = "YOUTRACK_BASE_URL")]
    youtrack_base_url: String,

    /// YouTrack permanent token (Authorization: Bearer ...)
    #[arg(long, env = "YOUTRACK_TOKEN")]
    youtrack_token: String,

    /// YouTrack Agile board id from URL, e.g. 65-52
    #[arg(long, env = "YOUTRACK_AGILE_ID")]
    agile_id: String,

    /// YouTrack sprint id from URL, e.g. 66-155467
    #[arg(long, env = "YOUTRACK_SPRINT_ID")]
    sprint_id: String,

    /// Custom field name used for ticket state (defaults to 'State')
    #[arg(long, env = "YOUTRACK_STATE_FIELD", default_value = "State")]
    state_field: String,

    /// State value considered "open" (defaults to 'Open')
    #[arg(long, env = "YOUTRACK_OPEN_VALUE", default_value = "Open")]
    open_value: String,

    /// Only print what would be created
    #[arg(long)]
    dry_run: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let mut youtrack_base =
        Url::parse(&args.youtrack_base_url).context("invalid YOUTRACK_BASE_URL")?;
    if !youtrack_base.as_str().ends_with('/') {
        youtrack_base =
            Url::parse(&format!("{}/", youtrack_base.as_str())).context("invalid base URL")?;
    }

    let deployment = DeploymentImpl::new().await?;
    let pool = &deployment.db().pool;

    let summary = sync_open_sprint_issues_to_todo(
        pool,
        args.project_id,
        youtrack_base,
        YouTrackAuthToken(args.youtrack_token),
        &args.agile_id,
        &args.sprint_id,
        &args.state_field,
        &args.open_value,
        args.dry_run,
    )
    .await?;

    tracing::info!("Done. {:?}", summary);

    Ok(())
}
