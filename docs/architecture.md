# Architecture

## Overview

Kinetic-rs is built around three core concepts:

1. **Agents** - LLM-powered units that can use tools and follow instructions
2. **Workflows** - Compositions of agents with execution strategies
3. **Tools** - Functions that agents can call to interact with external systems

```
┌─────────────────────────────────────────────────────────────┐
│                        Workflow                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
│  │   Agent 1   │→ │   Agent 2   │→ │   Agent 3   │         │
│  │  (Fetcher)  │  │ (Processor) │  │ (Summarizer)│         │
│  └──────┬──────┘  └─────────────┘  └─────────────┘         │
│         │                                                    │
│  ┌──────▼──────┐                                            │
│  │    Tools    │                                            │
│  │ ┌─────────┐ │                                            │
│  │ │  Jira   │ │                                            │
│  │ │ GitHub  │ │                                            │
│  │ │  MCP    │ │                                            │
│  │ └─────────┘ │                                            │
│  └─────────────┘                                            │
└─────────────────────────────────────────────────────────────┘
```

## Components

### Agent Types

| Type | Description |
|------|-------------|
| `LLMAgent` | Single agent with LLM, tools, and instructions |
| `ReActAgent` | Explicit Thought → Action → Observation reasoning loop |
| `GraphAgent` | DAG-based execution with conditional branching and state |

### Execution Flow

```
┌──────────┐     ┌──────────┐     ┌──────────┐
│  Input   │────▶│  Agent   │────▶│  Output  │
└──────────┘     └────┬─────┘     └──────────┘
                      │
                      ▼
              ┌───────────────┐
              │  LLM Provider │
              │ (Gemini/OpenAI)│
              └───────┬───────┘
                      │
         ┌────────────┼────────────┐
         ▼            ▼            ▼
    ┌─────────┐ ┌─────────┐ ┌─────────┐
    │  Tool   │ │  Tool   │ │  Tool   │
    │  Call   │ │  Call   │ │  Call   │
    └─────────┘ └─────────┘ └─────────┘
```

### Agent Turn Loop

Each `LLMAgent` runs a turn-based loop:

1. Send conversation history to LLM
2. Receive response (text or function calls)
3. If text → return as final output
4. If function calls → execute ALL tools, add results to history
5. Repeat until text response or max turns (10)

```rust
for turn in 0..max_turns {
    let response = model.generate_content(&history, tools).await?;

    // Check for text response (final answer)
    if let Some(text) = response.get_text() {
        return Ok(text);
    }

    // Execute all function calls
    for (name, args) in response.get_function_calls() {
        let result = tools.execute(name, args).await;
        history.push(FunctionResponse { name, result });
    }
}
```

## Directory Structure

```
kinetic-rs/
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Library exports
│   ├── adk/                  # Agent Development Kit
│   │   ├── agent/           # Agent implementations
│   │   │   ├── mod.rs       # Agent trait
│   │   │   ├── llm.rs       # Standard LLM agent
│   │   │   └── react.rs     # ReAct agent
│   │   ├── model/           # Model implementations
│   │   │   ├── mod.rs       # Model trait
│   │   │   ├── gemini.rs    # Gemini
│   │   │   ├── openai.rs    # OpenAI
│   │   │   └── anthropic.rs # Anthropic
│   │   ├── tool.rs          # Tool trait
│   │   └── error.rs         # Typed error handling
│   └── kinetic/
│       ├── workflow/
│       │   ├── graph/       # Graph workflow execution
│       │   │   ├── executor.rs  # GraphAgent implementation
│       │   │   ├── normalizer.rs # Workflow → Graph conversion
│       │   │   └── types.rs     # Graph node definitions
│       │   ├── condition/   # Conditional expressions
│       │   ├── state/       # Workflow state management
│       │   ├── loader.rs    # YAML parsing
│       │   ├── builder.rs   # Workflow construction
│       │   └── registry.rs  # Tool registry
│       ├── tools/
│       │   ├── github.rs    # GitHub API tools
│       │   ├── jira.rs      # Jira API tools
│       │   └── search.rs    # Web search tools
│       └── mcp/
│           ├── manager.rs   # MCP server lifecycle
│           └── tool.rs      # MCP tool wrapper
├── agents/                   # Reusable agent definitions
├── examples/                 # Example workflows
├── tests/                    # Integration tests
└── schemas/                  # JSON Schema for validation
```

## Model Provider Abstraction

The `Model` trait abstracts LLM providers:

```rust
#[async_trait]
pub trait Model: Send + Sync {
    async fn generate_content(
        &self,
        history: &[Content],
        config: Option<&GenerationConfig>,
        tools: Option<&[Arc<dyn Tool>]>,
    ) -> Result<Content, Box<dyn Error + Send + Sync>>;
}
```

### Provider Selection

Providers are selected in order:
1. Explicit `provider` in YAML
2. `MODEL_PROVIDER` environment variable
3. Inferred from model name prefix:
   - `gemini-*` → Gemini
   - `gpt-*`, `o1-*` → OpenAI
   - `claude-*` → Anthropic
   - `deepseek-*` → DeepSeek

## Tool System

All tools implement the `Tool` trait:

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;           // Returns reference (no allocation)
    fn description(&self) -> &str;    // Returns reference
    fn schema(&self) -> &Value;       // Returns reference to JSON schema
    async fn execute(&self, input: Value) -> Result<Value, Error>;
}
```

### Tool Registration Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                      Tool Registry                               │
│                                                                  │
│  ┌──────────────────────┐    ┌──────────────────────┐          │
│  │    Native Tools      │    │      MCP Tools       │          │
│  │  ┌────────────────┐  │    │  ┌────────────────┐  │          │
│  │  │ fetch_pull_req │  │    │  │ server:tool_a  │  │          │
│  │  │ get_jira_issue │  │    │  │ server:tool_b  │  │          │
│  │  │ brave_search   │  │    │  │ server:tool_c  │  │          │
│  │  └────────────────┘  │    │  └────────────────┘  │          │
│  └──────────────────────┘    └──────────────────────┘          │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
                    Agent requests tool by name
```

### Native Tools

Native tools are Rust implementations compiled into the binary. They're registered at startup based on available environment variables:

```rust
// Registration in main.rs
if let Ok(github_tools) = github::create_tools() {
    for tool in github_tools {
        registry.register(tool).await;
    }
}
```

**Characteristics:**
- Fast execution (no IPC overhead)
- Type-safe parameter handling
- Graceful degradation (not registered if env vars missing)

**Available Native Tools:**

| Category | Tools | Required Env Vars |
|----------|-------|-------------------|
| GitHub | `fetch_pull_request`, `get_pull_request_diff`, `list_merged_prs` | `GITHUB_TOKEN`, `GITHUB_ORG`, `GITHUB_REPO` |
| Jira | `get_jira_issue`, `search_jira_issues`, `get_my_project_issues`, `get_assigned_issues` | `JIRA_BASE_URL`, `JIRA_API_TOKEN` |
| Search | `brave_search` | `BRAVE_API_KEY` |

### MCP Tools

MCP (Model Context Protocol) tools come from external servers spawned as subprocesses. This enables:
- Using any MCP-compatible tool server
- Dynamic tool discovery
- Language-agnostic tool implementations

```
┌─────────────┐     stdio      ┌─────────────────────┐
│  Kinetic    │◄──────────────►│   MCP Server        │
│  (Client)   │   JSON-RPC     │ (npx, python, etc)  │
└─────────────┘                └─────────────────────┘
```

**MCP Server Configuration:**

```yaml
mcp_servers:
  - name: "sqlite"
    command: "npx"
    args: ["-y", "@modelcontextprotocol/server-sqlite", "data.db"]

  - name: "filesystem"
    command: "python"
    args: ["-m", "mcp_server_filesystem", "/path/to/dir"]
```

**Tool Namespacing:**

MCP tools are namespaced to avoid collisions:
- Server name: `myserver`
- Tool name: `mytool`
- Full name in workflow: `myserver:mytool`

**MCP Tool Lifecycle:**

```
1. Workflow loads with mcp_servers config
          │
          ▼
2. McpServiceManager spawns server subprocess
          │
          ▼
3. Client connects via stdio, sends initialize
          │
          ▼
4. Client calls tools/list to discover tools
          │
          ▼
5. Each tool wrapped as McpTool, registered in registry
          │
          ▼
6. Agent can now call "server:tool_name"
```

### Tool Execution During Agent Turn

When the LLM requests a tool call:

```rust
// Agent receives function call from LLM
Part::FunctionCall { name, args } => {
    // Look up tool in registry
    let tool = registry.get(&name).await;

    // Execute (works same for native or MCP)
    let result = tool.execute(args).await;

    // Add result to conversation history
    history.push(FunctionResponse { name, response: result });
}
```

Both native and MCP tools are treated identically by the agent - the abstraction is transparent.

## Workflow Loading

```
YAML File
    │
    ▼
┌───────────────┐
│ WorkflowLoader│  Parses YAML into WorkflowDefinition
└───────┬───────┘
        │
        ▼
┌───────────────┐
│    Builder    │  Resolves references, creates agents
└───────┬───────┘
        │
        ▼
┌───────────────┐
│  Arc<dyn Agent>│  Ready to execute
└───────────────┘
```

## Error Handling

- Tool failures return error JSON, don't crash the agent
- Missing tools log warnings, continue execution
- Max turns prevent infinite loops
- MCP server failures are logged but don't block other tools

