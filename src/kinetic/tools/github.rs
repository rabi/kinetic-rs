use crate::adk::tool::Tool;
use async_trait::async_trait;
use octocrab::Octocrab;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::error::Error;
use std::sync::Arc;

// --- Static schemas (computed once, returned by reference) ---

static FETCH_PR_SCHEMA: Lazy<Value> = Lazy::new(|| {
    json!({
        "type": "object",
        "properties": {
            "pr_number": {
                "type": "integer",
                "description": "The pull request number"
            },
            "owner": {
                "type": "string",
                "description": "Repository owner/org (optional, defaults to GITHUB_ORG env var)"
            },
            "repo": {
                "type": "string",
                "description": "Repository name (optional, defaults to GITHUB_REPO env var)"
            }
        },
        "required": ["pr_number"]
    })
});

static GET_DIFF_SCHEMA: Lazy<Value> = Lazy::new(|| {
    json!({
        "type": "object",
        "properties": {
            "pr_number": {
                "type": "integer",
                "description": "The pull request number"
            },
            "owner": {
                "type": "string",
                "description": "Repository owner/org (optional)"
            },
            "repo": {
                "type": "string",
                "description": "Repository name (optional)"
            }
        },
        "required": ["pr_number"]
    })
});

static LIST_MERGED_PRS_SCHEMA: Lazy<Value> = Lazy::new(|| {
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
});

static GET_COMMENTS_SCHEMA: Lazy<Value> = Lazy::new(|| {
    json!({
        "type": "object",
        "properties": {
            "pr_number": {
                "type": "integer",
                "description": "The pull request number"
            },
            "owner": {
                "type": "string",
                "description": "Repository owner/org (optional)"
            },
            "repo": {
                "type": "string",
                "description": "Repository name (optional)"
            }
        },
        "required": ["pr_number"]
    })
});

// --- Fetch Pull Request ---

#[derive(Debug, Serialize, Deserialize)]
pub struct FetchPRArgs {
    pub pr_number: u64,
    /// Optional owner/org - falls back to GITHUB_ORG env var
    pub owner: Option<String>,
    /// Optional repo name - falls back to GITHUB_REPO env var
    pub repo: Option<String>,
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
    fn name(&self) -> &str {
        "fetch_pull_request"
    }

    fn description(&self) -> &str {
        "Fetches a pull request by number. Returns PR details including title, body, description, and changed files. Can optionally specify owner/repo."
    }

    fn schema(&self) -> &Value {
        &FETCH_PR_SCHEMA
    }

    async fn execute(&self, input: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let args: FetchPRArgs = serde_json::from_value(input)?;

        // Use provided owner/repo or fall back to defaults
        let owner = args.owner.as_ref().unwrap_or(&self.owner);
        let repo = args.repo.as_ref().unwrap_or(&self.repo);

        let pr = self.octocrab.pulls(owner, repo).get(args.pr_number).await?;
        let files = self
            .octocrab
            .pulls(owner, repo)
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
    pub owner: Option<String>,
    pub repo: Option<String>,
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
    fn name(&self) -> &str {
        "get_pull_request_diff"
    }

    fn description(&self) -> &str {
        "Gets the full diff for a pull request showing all code changes. Can optionally specify owner/repo."
    }

    fn schema(&self) -> &Value {
        &GET_DIFF_SCHEMA
    }

    async fn execute(&self, input: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let args: GetDiffArgs = serde_json::from_value(input)?;

        let owner = args.owner.as_ref().unwrap_or(&self.owner);
        let repo = args.repo.as_ref().unwrap_or(&self.repo);

        let files = self
            .octocrab
            .pulls(owner, repo)
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
    fn name(&self) -> &str {
        "list_merged_prs"
    }

    fn description(&self) -> &str {
        "Lists pull requests that were merged within the specified number of days."
    }

    fn schema(&self) -> &Value {
        &LIST_MERGED_PRS_SCHEMA
    }

    async fn execute(&self, input: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let args: ListMergedPRsArgs = serde_json::from_value(input)?;

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
            prs.push(MergedPRInfo {
                number: issue.number,
                title: issue.title,
                author: issue.user.login,
                merged_at: issue.closed_at.map(|d| d.to_rfc3339()).unwrap_or_default(),
                merge_sha: "unknown".to_string(),
            });
        }

        let result = ListMergedPRsResult { prs };
        Ok(serde_json::to_value(result)?)
    }
}

// --- Get Pull Request Comments ---

#[derive(Debug, Serialize, Deserialize)]
pub struct GetCommentsArgs {
    pub pr_number: u64,
    pub owner: Option<String>,
    pub repo: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PRComment {
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub comment_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetCommentsResult {
    pub comments: Vec<PRComment>,
}

pub struct GetPRCommentsTool {
    octocrab: Arc<Octocrab>,
    owner: String,
    repo: String,
}

impl GetPRCommentsTool {
    pub fn new(octocrab: Arc<Octocrab>, owner: String, repo: String) -> Self {
        Self {
            octocrab,
            owner,
            repo,
        }
    }
}

#[async_trait]
impl Tool for GetPRCommentsTool {
    fn name(&self) -> &str {
        "get_pull_request_comments"
    }

    fn description(&self) -> &str {
        "Gets all comments on a pull request including review comments and issue comments. Can optionally specify owner/repo."
    }

    fn schema(&self) -> &Value {
        &GET_COMMENTS_SCHEMA
    }

    async fn execute(&self, input: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let args: GetCommentsArgs = serde_json::from_value(input)?;

        let owner = args.owner.as_ref().unwrap_or(&self.owner);
        let repo = args.repo.as_ref().unwrap_or(&self.repo);

        let mut all_comments = Vec::new();

        // Fetch issue comments (general PR comments)
        let issue_comments = self
            .octocrab
            .issues(owner, repo)
            .list_comments(args.pr_number)
            .send()
            .await?;

        for comment in issue_comments.items {
            all_comments.push(PRComment {
                author: comment.user.login,
                body: comment.body.unwrap_or_default(),
                created_at: comment.created_at.to_rfc3339(),
                comment_type: "issue".to_string(),
            });
        }

        // Fetch review comments (inline code comments)
        let review_comments = self
            .octocrab
            .pulls(owner, repo)
            .list_comments(Some(args.pr_number))
            .send()
            .await?;

        for comment in review_comments.items {
            let author = comment
                .user
                .map(|u| u.login)
                .unwrap_or_else(|| "unknown".to_string());
            all_comments.push(PRComment {
                author,
                body: comment.body,
                created_at: comment.created_at.to_rfc3339(),
                comment_type: "review".to_string(),
            });
        }

        // Sort by created_at
        all_comments.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        let result = GetCommentsResult {
            comments: all_comments,
        };
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
        Arc::new(GetPRCommentsTool::new(
            octocrab.clone(),
            owner.clone(),
            repo.clone(),
        )),
        Arc::new(ListMergedPRsTool::new(octocrab, owner, repo)),
    ])
}
