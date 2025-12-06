# User Guide

## Table of Contents

1. [Creating Agents](#creating-agents)
2. [Building Workflows](#building-workflows)
3. [Using Tools](#using-tools)
4. [Environment Configuration](#environment-configuration)
5. [Debugging](#debugging)

---

## Creating Agents

Agents are the building blocks of workflows. Each agent has:
- **Instructions**: System prompt defining behavior
- **Model**: LLM configuration
- **Tools**: Functions the agent can call

### Basic Agent

```yaml
# yaml-language-server: $schema=../schemas/workflow.schema.json
kind: Direct
name: MyAgent
description: "What this agent does"

agent:
  name: MyAgent
  description: "Brief description"
  instructions: |
    You are a helpful assistant.
    
    Your task is to:
    1. Understand the user's request
    2. Use available tools if needed
    3. Provide a clear response
  model:
    kind: llm
  tools: []
```

### Agent with Tools

```yaml
agent:
  name: JiraFetcher
  description: "Fetches Jira issues"
  instructions: |
    Fetch issues from Jira using the provided tools.
    Call get_my_project_issues with the project key.
  model:
    kind: llm
  tools:
    - get_my_project_issues
    - get_jira_issue_details
```

### Model Configuration

```yaml
model:
  kind: llm
  # Optional: Override provider (auto-detected from model name)
  provider: Gemini
  # Optional: Override model (defaults to MODEL_NAME env var)
  model_name: gemini-2.0-flash
  # Optional: Model parameters
  parameters:
    temperature: 0.2
    max_tokens: 4000
```

---

## Building Workflows

### Sequential Workflow

Agents run one after another. Output of each becomes input to the next.

```yaml
kind: Composite
name: FetchAndSummarize
description: "Fetch data then summarize"

workflow:
  execution: sequential
  agents:
    - file: agents/fetcher.yaml
    - file: agents/summarizer.yaml
```

### Parallel Workflow

Agents run concurrently. All receive the same input, outputs are combined.

```yaml
kind: Composite
name: ParallelFetch
description: "Fetch multiple data sources"

workflow:
  execution: parallel
  agents:
    - file: agents/fetch_metadata.yaml
    - file: agents/fetch_diff.yaml
```

### Loop Workflow

Agents repeat until max iterations. Useful for iterative refinement.

```yaml
kind: Composite
name: WriteAndEdit
description: "Iterative writing with feedback"

workflow:
  execution: loop
  max_iterations: 3
  agents:
    - file: agents/writer.yaml
    - file: agents/editor.yaml
```

### Nested Workflows

Workflows can reference other workflows:

```yaml
workflow:
  execution: sequential
  agents:
    # Reference a parallel workflow
    - file: examples/parallel_fetch.yaml
    # Then process the combined output
    - file: agents/processor.yaml
```

---

## Using Tools

### Available Native Tools

#### GitHub Tools

| Tool | Description | Required Env Vars |
|------|-------------|-------------------|
| `fetch_pull_request` | Get PR metadata | `GITHUB_TOKEN`, `GITHUB_ORG`, `GITHUB_REPO` |
| `get_pull_request_diff` | Get PR code diff | Same as above |
| `list_merged_prs` | List recently merged PRs | Same as above |

#### Jira Tools

| Tool | Description | Required Env Vars |
|------|-------------|-------------------|
| `get_jira_issue` | Get issue details | `JIRA_BASE_URL`, `JIRA_API_TOKEN` |
| `get_jira_issue_details` | Get issue with comments | Same as above |
| `search_jira_issues` | Search with JQL | Same as above |
| `get_my_project_issues` | Get your assigned issues | Same as above |
| `get_assigned_issues` | Get all your issues | Same as above |

#### Search Tools

| Tool | Description | Required Env Vars |
|------|-------------|-------------------|
| `brave_search` | Web search | `BRAVE_API_KEY` |

### MCP Tools

Connect to any MCP server:

```yaml
mcp_servers:
  - name: "sqlite"
    command: "npx"
    args: ["-y", "@modelcontextprotocol/server-sqlite", "database.db"]

agent:
  tools:
    - "sqlite:list_tables"
    - "sqlite:run_query"
```

---

## Environment Configuration

### Required Variables

```bash
# At minimum, you need an LLM API key
GEMINI_API_KEY=your-key
```

### Model Configuration

```bash
# Model selection (in order of precedence)
MODEL_NAME=gemini-2.0-flash      # Explicit model name
GEMINI_MODEL=gemini-2.0-flash    # Provider-specific fallback

# Provider override (usually auto-detected)
MODEL_PROVIDER=Gemini
```

### Tool Credentials

```bash
# GitHub
GITHUB_TOKEN=ghp_xxxxx
GITHUB_ORG=openstack-k8s-operators
GITHUB_REPO=edpm-ansible

# Jira Cloud
JIRA_BASE_URL=https://company.atlassian.net
JIRA_EMAIL=you@company.com
JIRA_API_TOKEN=your-api-token

# Jira Data Center (PAT)
JIRA_BASE_URL=https://jira.company.com
JIRA_API_TOKEN=your-pat-token
JIRA_AUTH_TYPE=bearer

# Web Search
BRAVE_API_KEY=your-brave-api-key
```

---

## Debugging

### Enable Logging

```bash
# Info level - see tool registration and agent turns
RUST_LOG=info cargo run -- workflow --file examples/my_workflow.yaml --input "test"

# Debug level - see API requests and responses
RUST_LOG=debug cargo run -- workflow --file examples/my_workflow.yaml --input "test"
```

### Common Issues

#### "Tool not found: tool_name"

The tool isn't registered. Check:
- Required environment variables are set
- Tool name matches exactly (case-sensitive)

#### "Missing field `provider`"

Old YAML format. The `provider` field is now optional. Just use:
```yaml
model:
  kind: llm
```

#### Agent returns empty response

The LLM returned empty text. This can happen when:
- Instructions are unclear
- Input is too short or ambiguous
- Tool calls failed silently

Check logs for tool execution errors.

#### "Max turns reached"

The agent hit the 10-turn limit without producing a text response. This usually means:
- The agent is stuck in a tool-calling loop
- Instructions don't tell it when to stop and summarize

Add explicit instructions like:
```yaml
instructions: |
  After fetching the data, summarize it and STOP.
  Do not call tools again after getting results.
```

### Validating YAML

Install the YAML extension in VS Code/Cursor for schema validation:

```yaml
# Add this to the top of your YAML file
# yaml-language-server: $schema=../schemas/workflow.schema.json
```

Or configure in `.vscode/settings.json`:
```json
{
  "yaml.schemas": {
    "./schemas/workflow.schema.json": ["agents/*.yaml", "examples/*.yaml"]
  }
}
```

