// SPDX-License-Identifier: MIT

use crate::adk::tool::Tool;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::error::Error;

// --- Static schema ---

static BRAVE_SEARCH_SCHEMA: Lazy<Value> = Lazy::new(|| {
    json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "The search query"
            },
            "count": {
                "type": "integer",
                "description": "Number of results to return (default 10, max 20)"
            },
            "freshness": {
                "type": "string",
                "description": "Freshness filter: pd (past day), pw (past week), pm (past month), py (past year)"
            }
        },
        "required": ["query"]
    })
});

#[derive(Debug, Serialize, Deserialize)]
pub struct BraveSearchArgs {
    pub query: String,
    #[serde(default)]
    pub count: Option<u32>,
    #[serde(default)]
    pub freshness: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BraveSearchResult {
    pub results: Vec<SearchResult>,
    pub query: String,
}

pub struct BraveSearchTool {
    client: Client,
    api_key: String,
}

impl BraveSearchTool {
    pub fn new() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let api_key = env::var("BRAVE_API_KEY").map_err(|_| "BRAVE_API_KEY must be set")?;
        Ok(Self {
            client: Client::new(),
            api_key,
        })
    }
}

#[async_trait]
impl Tool for BraveSearchTool {
    fn name(&self) -> &str {
        "brave_search"
    }

    fn description(&self) -> &str {
        "Searches the web using Brave Search API. Returns relevant search results with titles, URLs, and descriptions."
    }

    fn schema(&self) -> &Value {
        &BRAVE_SEARCH_SCHEMA
    }

    async fn execute(&self, input: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let args: BraveSearchArgs = serde_json::from_value(input)?;

        let count = args.count.unwrap_or(10).min(20);

        let mut url = reqwest::Url::parse("https://api.search.brave.com/res/v1/web/search")?;
        url.query_pairs_mut()
            .append_pair("q", &args.query)
            .append_pair("count", &count.to_string());

        if let Some(freshness) = &args.freshness {
            url.query_pairs_mut().append_pair("freshness", freshness);
        }

        let resp = self
            .client
            .get(url)
            .header("Accept", "application/json")
            .header("X-Subscription-Token", &self.api_key)
            .send()
            .await?;

        if !resp.status().is_success() {
            let text = resp.text().await?;
            return Err(format!("Brave API error: {}", text).into());
        }

        let body: Value = resp.json().await?;

        let results_json = body
            .get("web")
            .and_then(|w| w.get("results"))
            .ok_or("Invalid response format: missing web.results")?;

        let results: Vec<SearchResult> = serde_json::from_value(results_json.clone())?;

        let result = BraveSearchResult {
            results,
            query: args.query,
        };

        Ok(serde_json::to_value(result)?)
    }
}
