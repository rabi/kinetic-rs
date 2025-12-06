use crate::adk::tool::Tool;
use async_trait::async_trait;
use octocrab::Octocrab;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::error::Error;
use std::sync::Arc;

// --- Fetch Pull Request ---

#[derive(Debug, Serialize, Deserialize)]
pub struct FetchPRArgs {
    pub pr_number: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FetchPRResult {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub author: String,
    pub files: Vec<String>,
    pub additions: u64,
    pub deletions: u64,
}

pub struct FetchPRTool {
    octocrab: Arc<Octocrab>,
    owner: String,
    repo: String,
}

impl FetchPRTool {
    pub fn new(octocrab: Arc<Octocrab>, owner: String, repo: String) -> Self {
        Self {
            octocrab,
            owner,
            repo,
        }
    }
}

#[async_trait]
impl Tool for FetchPRTool {
    fn name(&self) -> String {
        "fetch_pull_request".to_string()
    }

    fn description(&self) -> String {
        "Fetches a pull request by number. Returns PR details including title, body, description, and changed files.".to_string()
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pr_number": {
                    "type": "integer",
                    "description": "The pull request number"
                }
            },
            "required": ["pr_number"]
        })
    }

    async fn execute(&self, input: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let args: FetchPRArgs = serde_json::from_value(input)?;

        let pr = self
            .octocrab
            .pulls(&self.owner, &self.repo)
            .get(args.pr_number)
            .await?;
        let files = self
            .octocrab
            .pulls(&self.owner, &self.repo)
            .list_files(args.pr_number)
            .await?;

        let file_names: Vec<String> = files.items.into_iter().map(|f| f.filename).collect();

        let result = FetchPRResult {
            number: pr.number,
            title: pr.title.unwrap_or_default(),
            body: pr.body,
            state: format!(
                "{:?}",
                pr.state.unwrap_or(octocrab::models::IssueState::Open)
            ),
            author: pr
                .user
                .map(|u| u.login)
                .unwrap_or_else(|| "unknown".to_string()),
            files: file_names,
            additions: pr.additions.unwrap_or(0),
            deletions: pr.deletions.unwrap_or(0),
        };

        Ok(serde_json::to_value(result)?)
    }
}

// --- Get Pull Request Diff ---

#[derive(Debug, Serialize, Deserialize)]
pub struct GetDiffArgs {
    pub pr_number: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetDiffResult {
    pub diff: String,
}

pub struct GetDiffTool {
    octocrab: Arc<Octocrab>,
    owner: String,
    repo: String,
}

impl GetDiffTool {
    pub fn new(octocrab: Arc<Octocrab>, owner: String, repo: String) -> Self {
        Self {
            octocrab,
            owner,
            repo,
        }
    }
}

#[async_trait]
impl Tool for GetDiffTool {
    fn name(&self) -> String {
        "get_pull_request_diff".to_string()
    }

    fn description(&self) -> String {
        "Gets the full diff for a pull request showing all code changes.".to_string()
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pr_number": {
                    "type": "integer",
                    "description": "The pull request number"
                }
            },
            "required": ["pr_number"]
        })
    }

    async fn execute(&self, input: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let args: GetDiffArgs = serde_json::from_value(input)?;

        // Octocrab doesn't have a direct "get diff" method that returns the raw diff string easily for PRs in the high-level API
        // But we can use the media type header to request diff
        // Or just list files and get patches. The Go implementation lists files and concatenates patches.

        let files = self
            .octocrab
            .pulls(&self.owner, &self.repo)
            .list_files(args.pr_number)
            .await?;

        let mut diffs = Vec::new();
        for file in files.items {
            if let Some(patch) = file.patch {
                diffs.push(format!("File: {}\n{}\n", file.filename, patch));
            }
        }

        let result = GetDiffResult {
            diff: diffs.join("\n---\n\n"),
        };

        Ok(serde_json::to_value(result)?)
    }
}

// --- List Merged PRs ---

#[derive(Debug, Serialize, Deserialize)]
pub struct ListMergedPRsArgs {
    pub days: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MergedPRInfo {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub merged_at: String,
    pub merge_sha: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListMergedPRsResult {
    pub prs: Vec<MergedPRInfo>,
}

pub struct ListMergedPRsTool {
    octocrab: Arc<Octocrab>,
    owner: String,
    repo: String,
}

impl ListMergedPRsTool {
    pub fn new(octocrab: Arc<Octocrab>, owner: String, repo: String) -> Self {
        Self {
            octocrab,
            owner,
            repo,
        }
    }
}

#[async_trait]
impl Tool for ListMergedPRsTool {
    fn name(&self) -> String {
        "list_merged_prs".to_string()
    }

    fn description(&self) -> String {
        "Lists pull requests that were merged within the specified number of days.".to_string()
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "days": {
                    "type": "integer",
                    "description": "Number of days to look back"
                }
            },
            "required": ["days"]
        })
    }

    async fn execute(&self, input: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let args: ListMergedPRsArgs = serde_json::from_value(input)?;

        // Calculate date threshold
        // Note: This is a simplified implementation.
        // Octocrab's list_prs doesn't support complex filtering directly in the builder easily for "merged > date"
        // We might need to fetch recent PRs and filter client-side or use search API.
        // Using search API is better: `is:pr is:merged repo:owner/repo merged:>date`

        let date = chrono::Utc::now() - chrono::Duration::days(args.days as i64);
        let date_str = date.format("%Y-%m-%d").to_string();
        let query = format!(
            "is:pr is:merged repo:{}/{} merged:>{}",
            self.owner, self.repo, date_str
        );

        let page = self
            .octocrab
            .search()
            .issues_and_pull_requests(&query)
            .send()
            .await?;

        let mut prs = Vec::new();
        for issue in page.items {
            // Search returns issues, need to check if it's a PR (it should be due to is:pr)
            // But issue struct in octocrab doesn't have all PR fields like merged_at directly populated the same way?
            // Actually search returns Issue struct which has pull_request field if it is a PR.

            // For simplicity, we map what we have.
            prs.push(MergedPRInfo {
                number: issue.number,
                title: issue.title,
                author: issue.user.login,
                merged_at: issue.closed_at.map(|d| d.to_rfc3339()).unwrap_or_default(), // Search result uses closed_at for merged PRs usually
                merge_sha: "unknown".to_string(), // Search result doesn't have merge SHA
            });
        }

        let result = ListMergedPRsResult { prs };
        Ok(serde_json::to_value(result)?)
    }
}

// --- Factory ---

pub fn create_tools() -> Result<Vec<Arc<dyn Tool>>, Box<dyn Error + Send + Sync>> {
    let token = env::var("GITHUB_TOKEN").map_err(|_| "GITHUB_TOKEN must be set")?;
    let owner = env::var("GITHUB_ORG").map_err(|_| "GITHUB_ORG must be set")?;
    let repo = env::var("GITHUB_REPO").map_err(|_| "GITHUB_REPO must be set")?;

    let octocrab = Octocrab::builder().personal_token(token).build()?;
    let octocrab = Arc::new(octocrab);

    Ok(vec![
        Arc::new(FetchPRTool::new(
            octocrab.clone(),
            owner.clone(),
            repo.clone(),
        )),
        Arc::new(GetDiffTool::new(
            octocrab.clone(),
            owner.clone(),
            repo.clone(),
        )),
        Arc::new(ListMergedPRsTool::new(octocrab, owner, repo)),
    ])
}
