# Kinetic-rs

A Rust framework for building AI agent workflows with support for multi-agent orchestration, graph-based workflows, ReAct agents, tool integration, and MCP (Model Context Protocol) servers.

## Features

- **Multi-Agent Workflows**: Chain agents sequentially, run in parallel, or loop until completion
- **Graph Workflows**: DAG-based execution with conditional branching and state management
- **ReAct Agent**: Reasoning + Acting pattern with explicit thought/action/observation loop
- **Tool Integration**: Built-in GitHub, Jira, and web search tools
- **MCP Support**: Connect to any MCP-compatible tool server
- **YAML Configuration**: Define workflows declaratively with JSON Schema validation

> **Note**: Currently only **Google Gemini** is supported as the LLM provider. OpenAI and Anthropic support is planned.

## Quick Start

### Prerequisites

- Rust 1.70+
- Gemini API key (get one at [Google AI Studio](https://aistudio.google.com/))

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
GEMINI_API_KEY=your-api-key

# GitHub (for PR tools)
GITHUB_TOKEN=ghp_xxxxx
GITHUB_ORG=your-org
GITHUB_REPO=your-repo

# Jira (for issue tools)
JIRA_BASE_URL=https://your-instance.atlassian.net
JIRA_API_TOKEN=your-token
JIRA_EMAIL=your-email@example.com
```

### Running Workflows

```bash
# Run a workflow
cargo run -- workflow --file examples/jira_project_summary.yaml --input "PROJECT_KEY"

# Run with debug logging
RUST_LOG=info cargo run -- workflow --file examples/pr_review_composed.yaml --input "123"
```

### Using `just` (Task Runner)

```bash
cargo install just
just              # Show all commands
just build        # Build the project
just test         # Run all tests
just ci           # Run fmt, lint, and tests
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

### Graph (DAG-Based)

Graph workflows enable complex DAG execution with conditional branching:

```yaml
kind: Graph
name: IntentRouter
description: "Routes based on user intent"

graph:
  state:
    intent:
      type: string

  nodes:
    - id: classifier
      agent:
        name: Classifier
        description: "Determines user intent"
        instructions: "Classify the intent as 'bug', 'feature', or 'question'"
        model:
          kind: llm
        tools: []
      outputs:
        intent: "intent"

    - id: bug_handler
      depends_on: classifier
      when: "intent == 'bug'"
      agent:
        file: agents/bug_handler.yaml

    - id: feature_handler
      depends_on: classifier
      when: "intent == 'feature'"
      agent:
        file: agents/feature_handler.yaml
```

### ReAct Agent

Use the ReAct (Reasoning + Acting) pattern for complex tool-using tasks:

```yaml
kind: Direct
name: ReactResearcher

agent:
  name: Researcher
  description: "Research agent using ReAct pattern"
  executor: react  # Enable ReAct mode
  max_iterations: 10
  instructions: |
    Research the given topic using available tools.
    Think step by step about what information you need.
  model:
    kind: llm
  tools:
    - brave_search
    - get_jira_issue
```

## Agent Types

| Type | Description | Use Case |
|------|-------------|----------|
| `LLMAgent` | Standard agent with LLM and tools | Most workflows |
| `ReActAgent` | Explicit reasoning loop | Complex multi-step tasks |
| `SequentialAgent` | Runs sub-agents in order | Pipelines |
| `ParallelAgent` | Runs sub-agents concurrently | Independent tasks |
| `LoopAgent` | Iterates until done | Refinement loops |
| `GraphAgent` | DAG-based execution | Conditional workflows |

## Examples

| Workflow | Description |
|----------|-------------|
| `jira_project_summary.yaml` | Fetches Jira issues and summarizes them |
| `pr_review_composed.yaml` | Fetches PR details and performs code review |
| `parallel_workflow.yaml` | Fetches PR metadata and diff in parallel |
| `graph_intent_router.yaml` | Routes requests based on classified intent |
| `react_research.yaml` | Research with ReAct reasoning pattern |
| `iterative_writing.yaml` | Write-edit loop with feedback |

## Project Structure

```
kinetic-rs/
├── src/
│   ├── adk/                  # Agent Development Kit
│   │   ├── agent.rs          # Agent trait and implementations
│   │   ├── model.rs          # LLM abstraction
│   │   ├── tool.rs           # Tool trait
│   │   ├── error.rs          # Typed error handling
│   │   └── gemini.rs         # Gemini API
│   └── kinetic/
│       ├── workflow/
│       │   ├── graph/        # Graph workflow execution
│       │   ├── loader.rs     # YAML parsing
│       │   └── builder.rs    # Workflow construction
│       ├── tools/            # Native tools (GitHub, Jira, Search)
│       └── mcp/              # MCP server integration
├── agents/                   # Reusable agent definitions
├── examples/                 # Example workflows
├── tests/                    # Integration tests
└── schemas/                  # JSON Schema for validation
```

## Development

```bash
# Run tests
just test              # or: cargo test

# Run linter
just lint              # or: cargo clippy

# Format code
just fmt               # or: cargo fmt

# Run CI checks
just ci                # fmt + lint + test
```

## Documentation

- [Architecture](docs/architecture.md) - System design and components
- [User Guide](docs/user-guide.md) - Detailed usage instructions

## License

MIT
