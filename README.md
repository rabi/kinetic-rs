# Kinetic-rs

A Rust framework for building AI agent workflows with support for multi-agent orchestration, tool integration, and MCP (Model Context Protocol) servers.

## Features

- **Multi-Agent Workflows**: Chain agents sequentially, run in parallel, or loop until completion
- **Tool Integration**: Built-in GitHub, Jira, and web search tools
- **MCP Support**: Connect to any MCP-compatible tool server
- **Provider Agnostic**: Supports Gemini, OpenAI, Anthropic (extensible)
- **YAML Configuration**: Define workflows declaratively with JSON Schema validation

## Quick Start

### Prerequisites

- Rust 1.70+
- API keys for your chosen LLM provider

### Installation

```bash
git clone https://github.com/your-org/kinetic-rs.git
cd kinetic-rs
cargo build
```

### Configuration

Create a `.env` file:

```bash
# LLM Configuration
MODEL_NAME=gemini-2.0-flash
# Or use provider-specific vars:
# GEMINI_MODEL=gemini-2.0-flash
# OPENAI_MODEL=gpt-4o

# Optional: Explicit provider (auto-inferred from model name)
# MODEL_PROVIDER=Gemini

# Gemini API
GEMINI_API_KEY=your-api-key

# GitHub (for PR tools)
GITHUB_TOKEN=ghp_xxxxx
GITHUB_ORG=your-org
GITHUB_REPO=your-repo

# Jira (for issue tools)
JIRA_BASE_URL=https://your-instance.atlassian.net
JIRA_API_TOKEN=your-token
# For Atlassian Cloud, also set:
JIRA_EMAIL=your-email@example.com
# For Jira Data Center with PAT, set:
JIRA_AUTH_TYPE=bearer
```

### Running Workflows

```bash
# Run a workflow
cargo run -- workflow --file examples/jira_project_summary.yaml --input "PROJECT_KEY"

# Run a simple prompt
cargo run -- run --prompt "Hello, world!"

# With debug logging
RUST_LOG=info cargo run -- workflow --file examples/pr_review_composed.yaml --input "123"
```

## Workflow Types

### Direct (Single Agent)

```yaml
kind: Direct
name: MyAgent
description: "A single agent workflow"

agent:
  name: MyAgent
  description: "Does something useful"
  instructions: |
    You are a helpful assistant.
  model:
    kind: llm
  tools:
    - tool_name
```

### Composite (Multi-Agent)

```yaml
kind: Composite
name: MyWorkflow
description: "Multi-agent workflow"

workflow:
  execution: sequential  # or: parallel, loop
  agents:
    - file: agents/step1.yaml
    - file: agents/step2.yaml
```

## Examples

| Workflow | Description |
|----------|-------------|
| `jira_project_summary.yaml` | Fetches Jira issues and summarizes them |
| `pr_review_composed.yaml` | Fetches PR details and performs code review |
| `parallel_workflow.yaml` | Fetches PR metadata and diff in parallel |
| `iterative_writing.yaml` | Write-edit loop with feedback |

## Documentation

- [Architecture](docs/architecture.md) - System design and components
- [User Guide](docs/user-guide.md) - Detailed usage instructions
- [Schema Reference](schemas/README.md) - YAML schema documentation

## License

MIT

