use crate::adk::tool::Tool;
use async_trait::async_trait;
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::error::Error;

// --- Jira Client Helper ---

#[derive(Clone)]
struct JiraClient {
    client: Client,
    base_url: String,
    email: Option<String>,
    api_token: String,
    use_bearer: bool,
}

impl JiraClient {
    fn new() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let base_url = env::var("JIRA_BASE_URL").map_err(|_| "JIRA_BASE_URL must be set")?;
        let api_token = env::var("JIRA_API_TOKEN").map_err(|_| "JIRA_API_TOKEN must be set")?;

        // Check for explicit auth type override, or auto-detect based on JIRA_EMAIL
        // JIRA_AUTH_TYPE=bearer forces Bearer auth, JIRA_AUTH_TYPE=basic forces Basic auth
        let auth_type = env::var("JIRA_AUTH_TYPE").ok();
        let email = env::var("JIRA_EMAIL").ok().filter(|e| !e.is_empty());

        let use_bearer = match auth_type.as_deref() {
            Some("bearer") => true,
            Some("basic") => false,
            _ => email.is_none(), // Auto-detect: no email = bearer
        };

        log::info!(
            "Jira client: base_url={}, use_bearer={}, has_email={}",
            base_url,
            use_bearer,
            email.is_some()
        );

        Ok(Self {
            client: Client::new(),
            base_url,
            email,
            api_token,
            use_bearer,
        })
    }

    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, Box<dyn Error + Send + Sync>> {
        // Jira Cloud uses API v3, Jira Data Center/Server uses API v2
        let api_version = if self.use_bearer { "2" } else { "3" };
        let url = format!("{}/rest/api/{}/{}", self.base_url, api_version, path);

        let mut req = self
            .client
            .request(method, &url)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json");

        if self.use_bearer {
            req = req.bearer_auth(&self.api_token);
        } else {
            req = req.basic_auth(self.email.as_ref().unwrap(), Some(&self.api_token));
        }

        if let Some(b) = body {
            req = req.json(&b);
        }

        let resp = req.send().await?;

        if !resp.status().is_success() {
            let text = resp.text().await?;
            return Err(format!("Jira API error: {}", text).into());
        }

        Ok(resp.json().await?)
    }
}

// --- Get Issue Tool ---

#[derive(Debug, Serialize, Deserialize)]
pub struct GetIssueArgs {
    pub issue_key: String,
}

pub struct GetIssueTool {
    client: JiraClient,
}

impl GetIssueTool {
    fn new(client: JiraClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Tool for GetIssueTool {
    fn name(&self) -> String {
        "get_jira_issue".to_string()
    }

    fn description(&self) -> String {
        "Gets detailed information about a specific Jira issue by its key (e.g., 'PROJ-123')."
            .to_string()
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "issue_key": {
                    "type": "string",
                    "description": "The issue key (e.g. PROJ-123)"
                }
            },
            "required": ["issue_key"]
        })
    }

    async fn execute(&self, input: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let args: GetIssueArgs = serde_json::from_value(input)?;

        // Fetch issue with comments expanded
        let json = self
            .client
            .request(
                Method::GET,
                &format!("issue/{}?expand=renderedFields", args.issue_key),
                None,
            )
            .await?;

        // Simplified extraction
        let fields = json.get("fields").ok_or("Missing fields in response")?;

        // Extract comments
        let comments: Vec<Value> = fields
            .get("comment")
            .and_then(|c| c.get("comments"))
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|comment| {
                        json!({
                            "author": comment.get("author").and_then(|a| a.get("displayName")),
                            "body": comment.get("body"),
                            "created": comment.get("created"),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let result = json!({
            "key": json.get("key"),
            "summary": fields.get("summary"),
            "description": fields.get("description"),
            "status": fields.get("status").and_then(|s| s.get("name")),
            "assignee": fields.get("assignee").and_then(|a| a.get("displayName")),
            "priority": fields.get("priority").and_then(|p| p.get("name")),
            "issue_type": fields.get("issuetype").and_then(|t| t.get("name")),
            "comments": comments,
        });

        Ok(result)
    }
}

// --- Search Issues Tool ---

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchIssuesArgs {
    pub jql: String,
    #[serde(default)]
    pub max_results: Option<u32>,
}

pub struct SearchIssuesTool {
    client: JiraClient,
}

impl SearchIssuesTool {
    fn new(client: JiraClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Tool for SearchIssuesTool {
    fn name(&self) -> String {
        "search_jira_issues".to_string()
    }

    fn description(&self) -> String {
        "Searches for Jira issues using JQL (Jira Query Language).".to_string()
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "jql": {
                    "type": "string",
                    "description": "JQL query string"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Max results to return"
                }
            },
            "required": ["jql"]
        })
    }

    async fn execute(&self, input: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let args: SearchIssuesArgs = serde_json::from_value(input)?;

        let body = json!({
            "jql": args.jql,
            "maxResults": args.max_results.unwrap_or(10),
            "fields": ["summary", "status", "assignee", "priority"]
        });

        let json = self
            .client
            .request(Method::POST, "search", Some(body))
            .await?;

        // Simplified mapping
        let issues = json
            .get("issues")
            .and_then(|i| i.as_array())
            .ok_or("Missing issues in response")?;

        let mapped_issues: Vec<Value> = issues
            .iter()
            .map(|issue| {
                let fields = issue.get("fields").unwrap();
                json!({
                    "key": issue.get("key"),
                    "summary": fields.get("summary"),
                    "status": fields.get("status").and_then(|s| s.get("name")),
                })
            })
            .collect();

        Ok(json!({
            "total": json.get("total"),
            "issues": mapped_issues
        }))
    }
}

// --- Get Project Issues Tool ---

#[derive(Debug, Serialize, Deserialize)]
pub struct GetProjectIssuesArgs {
    pub project_key: String,
}

pub struct GetProjectIssuesTool {
    client: JiraClient,
}

impl GetProjectIssuesTool {
    fn new(client: JiraClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Tool for GetProjectIssuesTool {
    fn name(&self) -> String {
        "get_my_project_issues".to_string()
    }

    fn description(&self) -> String {
        "Fetches in-progress issues assigned to the current user for a specific project."
            .to_string()
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "project_key": {
                    "type": "string",
                    "description": "The project key (e.g. OSPRH)"
                }
            },
            "required": ["project_key"]
        })
    }

    async fn execute(&self, input: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let args: GetProjectIssuesArgs = serde_json::from_value(input)?;

        let jql = format!(
            "project = {} AND assignee = currentUser() AND statusCategory = 'In Progress' ORDER BY updated DESC",
            args.project_key
        );

        let body = json!({
            "jql": jql,
            "maxResults": 10,
            "fields": ["summary", "status", "priority", "updated"]
        });

        let json = self
            .client
            .request(Method::POST, "search", Some(body))
            .await?;

        // Simplified mapping
        let issues = json
            .get("issues")
            .and_then(|i| i.as_array())
            .ok_or("Missing issues in response")?;

        let mapped_issues: Vec<Value> = issues
            .iter()
            .map(|issue| {
                let fields = issue.get("fields").unwrap();
                json!({
                    "key": issue.get("key"),
                    "summary": fields.get("summary"),
                    "status": fields.get("status").and_then(|s| s.get("name")),
                    "priority": fields.get("priority").and_then(|p| p.get("name")),
                    "updated": fields.get("updated"),
                })
            })
            .collect();

        Ok(json!({
            "total": json.get("total"),
            "issues": mapped_issues
        }))
    }
}

// --- Get Assigned Issues Tool ---

pub struct GetAssignedIssuesTool {
    client: JiraClient,
}

impl GetAssignedIssuesTool {
    fn new(client: JiraClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Tool for GetAssignedIssuesTool {
    fn name(&self) -> String {
        "get_assigned_issues".to_string()
    }

    fn description(&self) -> String {
        "Fetches all in-progress issues assigned to the current user across all projects."
            .to_string()
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(&self, _input: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let jql =
            "assignee = currentUser() AND statusCategory = 'In Progress' ORDER BY updated DESC";

        let body = json!({
            "jql": jql,
            "maxResults": 20,
            "fields": ["summary", "status", "priority", "updated", "project"]
        });

        let json = self
            .client
            .request(Method::POST, "search", Some(body))
            .await?;

        // Simplified mapping
        let issues = json
            .get("issues")
            .and_then(|i| i.as_array())
            .ok_or("Missing issues in response")?;

        let mapped_issues: Vec<Value> = issues
            .iter()
            .map(|issue| {
                let fields = issue.get("fields").unwrap();
                json!({
                    "key": issue.get("key"),
                    "project": fields.get("project").and_then(|p| p.get("name")),
                    "summary": fields.get("summary"),
                    "status": fields.get("status").and_then(|s| s.get("name")),
                    "priority": fields.get("priority").and_then(|p| p.get("name")),
                    "updated": fields.get("updated"),
                })
            })
            .collect();

        Ok(json!({
            "total": json.get("total"),
            "issues": mapped_issues
        }))
    }
}

pub fn create_tools() -> Result<Vec<std::sync::Arc<dyn Tool>>, Box<dyn Error + Send + Sync>> {
    // Only create tools if credentials exist, otherwise return empty list (soft failure)
    if env::var("JIRA_BASE_URL").is_err() {
        return Ok(vec![]);
    }

    let client = JiraClient::new()?;
    // Cloning client is cheap if we wrap internal client in Arc, but here we just recreate or clone struct
    // Since Client is cheap to clone (Arc internally), this is fine.

    // Hack: JiraClient doesn't implement Clone, so we create new ones or change design.
    // For simplicity, let's just create new ones or make JiraClient cloneable.
    // Reqwest Client is cloneable.

    Ok(vec![
        std::sync::Arc::new(GetIssueTool::new(client.clone())),
        std::sync::Arc::new(SearchIssuesTool::new(client.clone())),
        std::sync::Arc::new(GetProjectIssuesTool::new(client.clone())),
        std::sync::Arc::new(GetAssignedIssuesTool::new(client)),
    ])
}
