# Graph-Based Workflow Design

> Design document for implementing LangGraph-inspired graph-based workflows in kinetic-rs

## Overview

This document describes the design for a unified graph-based workflow execution model. The core idea is that **all workflows are graphs**, with existing patterns (`Direct`, `Composite`) becoming syntactic sugar over the graph model.

### Goals

1. **Unified execution model** - Single `GraphAgent` executor for all workflow types
2. **Backwards compatible** - Existing `Direct` and `Composite` workflows continue to work
3. **Explicit state management** - Declare what data flows between nodes
4. **Conditional routing** - Route based on node outputs using `when` conditions
5. **Dependency-based definition** - Use `depends_on` instead of explicit edges

### Non-Goals (Future Work)

- Checkpointing/persistence
- Human-in-the-loop interrupts
- Streaming intermediate results
- Subgraph composition

---

## Architecture

### Conceptual Model

```
┌─────────────────────────────────────────────────────────────┐
│                      YAML Definition                         │
├─────────────────┬─────────────────┬─────────────────────────┤
│  kind: Direct   │ kind: Composite │     kind: Graph         │
│  (1 agent)      │ (seq/par/loop)  │  (nodes + depends_on)   │
└────────┬────────┴────────┬────────┴────────────┬────────────┘
         │                 │                     │
         └─────────────────┼─────────────────────┘
                           ▼
              ┌────────────────────────┐
              │   GraphWorkflowDef     │  ← Normalized internal representation
              │   (nodes + state)      │
              └───────────┬────────────┘
                          ▼
              ┌────────────────────────┐
              │      GraphAgent        │  ← Single executor for all
              │   (executes graph)     │
              └────────────────────────┘
```

### Kind Normalization

| YAML Kind | Normalization |
|-----------|---------------|
| `Direct` | Single-node graph |
| `Composite` (sequential) | Linear chain with `depends_on` |
| `Composite` (parallel) | Multiple nodes, no dependencies |
| `Composite` (loop) | Graph with back-edge |
| `Graph` | Native graph definition |

---

## YAML Schema

### Full Graph Workflow

```yaml
# yaml-language-server: $schema=../schemas/workflow.schema.json
kind: Graph
name: PRReviewPipeline
description: "Comprehensive PR review with conditional routing"

workflow:
  # 1. STATE SCHEMA - Declare what data to track
  state:
    pr_metadata:
      type: object
    has_security_files:
      type: boolean
    review_findings:
      type: array
      reducer: append      # How to merge multiple updates
    final_score:
      type: number
      reducer: max

  # 2. NODES - Define agents with dependencies
  nodes:
    - id: fetch_pr
      agent:
        name: PRFetcher
        instructions: "Fetch PR metadata and files"
        model: { kind: llm }
        tools: [get_pull_request]
      outputs:
        pr_metadata: "metadata"
        has_security_files: "has_security_files"

    - id: fetch_diff
      agent: { file: agents/diff_fetcher.yaml }
      outputs:
        diff_content: "diff"

    - id: security_review
      depends_on: [fetch_pr, fetch_diff]
      when: "has_security_files == true"
      agent:
        name: SecurityReviewer
        instructions: "Review security implications"
        model: { kind: llm }
        tools: []
      outputs:
        review_findings: "findings"    # Appends to array
        final_score: "security_score"

    - id: code_review
      depends_on: [fetch_pr, fetch_diff]
      agent: { file: agents/code_reviewer.yaml }
      outputs:
        review_findings: "findings"    # Appends to array
        final_score: "code_score"

    - id: summarize
      depends_on: [security_review, code_review]
      wait_for: any                    # Run when ANY dependency completes
      agent:
        name: Summarizer
        instructions: "Summarize all review findings"
        model: { kind: llm }
        tools: []
```

### Backwards-Compatible Syntax

```yaml
# Direct workflow - still works!
kind: Direct
name: SimpleAgent
agent:
  name: MyAgent
  instructions: "Do something"
  model: { kind: llm }
  tools: []

# Internally becomes:
# nodes: [{ id: main, agent: {...} }]
```

```yaml
# Composite sequential - still works!
kind: Composite
name: Pipeline
workflow:
  execution: sequential
  agents:
    - file: agents/step1.yaml
    - file: agents/step2.yaml
    - file: agents/step3.yaml

# Internally becomes:
# nodes:
#   - id: step1, agent: {...}
#   - id: step2, agent: {...}, depends_on: step1
#   - id: step3, agent: {...}, depends_on: step2
```

```yaml
# Composite parallel - still works!
kind: Composite
name: ParallelWork
workflow:
  execution: parallel
  agents:
    - file: agents/worker1.yaml
    - file: agents/worker2.yaml
    - file: agents/worker3.yaml

# Internally becomes:
# nodes:
#   - id: worker1, agent: {...}
#   - id: worker2, agent: {...}
#   - id: worker3, agent: {...}
# (no depends_on = all run in parallel)
```

---

## State Management

### State Schema

```yaml
state:
  <field_name>:
    type: string | number | boolean | array | object
    reducer: overwrite | append | max | min | merge
    default: <value>
```

### Reducers

| Reducer | Behavior | Use Case |
|---------|----------|----------|
| `overwrite` | Replace value (default) | Single value fields |
| `append` | Add to array | Collecting results |
| `max` | Keep maximum | Scores, priorities |
| `min` | Keep minimum | Costs, errors |
| `merge` | Deep merge objects | Aggregating metadata |

### Output Mapping

Nodes declare how their output maps to state:

```yaml
nodes:
  - id: analyzer
    agent: { ... }
    outputs:
      # state_field: json_path
      intent: "intent"              # Simple field
      confidence: "scores.main"     # Nested path
      tags: "metadata.tags"         # Array field
```

### State Flow

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Node A    │────▶│   Node B    │────▶│   Node C    │
└─────────────┘     └─────────────┘     └─────────────┘
      │                   │                   │
      ▼                   ▼                   ▼
   outputs:            outputs:            outputs:
   intent: "x"         score: 0.9          result: "y"
      │                   │                   │
      └───────────────────┼───────────────────┘
                          ▼
              ┌─────────────────────┐
              │   WorkflowState     │
              ├─────────────────────┤
              │ intent: "x"         │
              │ score: 0.9          │
              │ result: "y"         │
              └─────────────────────┘
```

---

## Structured Output

Modern LLM APIs support **constrained/structured output**, guaranteeing the model returns valid JSON matching a schema. This makes `outputs` mapping reliable without parsing heuristics.

### Provider Support

| Provider | Feature | API Field |
|----------|---------|-----------|
| **Gemini** | Response Schema | `generationConfig.response_schema` |
| **OpenAI** | JSON Mode / Strict | `response_format.json_schema` |
| **Anthropic** | Tool Use | Define output as tool call |

### YAML Syntax

Nodes can define an `output_schema` to enforce structured output:

```yaml
nodes:
  - id: classifier
    agent:
      name: IntentClassifier
      instructions: "Classify the user query intent and confidence"
      model: { kind: llm }
      tools: []

    # NEW: JSON Schema for structured output
    output_schema:
      type: object
      properties:
        intent:
          type: string
          enum: [search, code, chat, question]
          description: "The classified intent"
        confidence:
          type: number
          minimum: 0
          maximum: 1
          description: "Confidence score"
        reasoning:
          type: string
          description: "Brief explanation"
      required: [intent, confidence]

    # Maps structured fields to state
    outputs:
      intent: "intent"
      confidence: "confidence"
```

### How It Works

```
┌──────────────────────────────────────────────────────────────┐
│                     Without Structured Output                 │
├──────────────────────────────────────────────────────────────┤
│  Instructions: "Output JSON with intent and confidence"      │
│  LLM Output: "Based on my analysis, the intent is search..." │
│  Result: ❌ Parsing fails, need fallback heuristics          │
└──────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────┐
│                     With Structured Output                    │
├──────────────────────────────────────────────────────────────┤
│  output_schema: { intent: string, confidence: number }        │
│  LLM Output: {"intent": "search", "confidence": 0.92}        │
│  Result: ✅ Guaranteed valid JSON matching schema            │
└──────────────────────────────────────────────────────────────┘
```

### Implementation

```rust
// workflow/graph/executor.rs

impl GraphAgent {
    async fn execute_node(
        &self,
        node: &CompiledNode,
        state: &WorkflowState
    ) -> Result<Value> {
        // Build generation config with structured output
        let config = if let Some(schema) = &node.output_schema {
            Some(GenerationConfig {
                response_mime_type: Some("application/json".to_string()),
                response_schema: Some(schema.clone()),
                ..Default::default()
            })
        } else {
            None
        };

        // Run agent with config
        let output = node.agent.run_with_config(input, config).await?;

        // Output is guaranteed to be valid JSON matching schema
        let json: Value = serde_json::from_str(&output)?;

        Ok(json)
    }
}
```

```rust
// adk/model.rs - Extended GenerationConfig

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationConfig {
    pub temperature: Option<f32>,
    pub max_output_tokens: Option<u32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,

    // NEW: Structured output
    pub response_mime_type: Option<String>,
    pub response_schema: Option<serde_json::Value>,
}
```

```rust
// adk/gemini.rs - Apply structured output config

if let Some(config) = config {
    if let Some(mime_type) = &config.response_mime_type {
        body["generationConfig"]["response_mime_type"] = json!(mime_type);
    }
    if let Some(schema) = &config.response_schema {
        body["generationConfig"]["response_schema"] = schema.clone();
    }
}
```

### Node Definition Update

```rust
// workflow/graph/types.rs

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodeDefinition {
    pub id: String,
    pub agent: AgentConfig,
    #[serde(default)]
    pub depends_on: DependsOn,
    pub when: Option<String>,

    /// JSON Schema for structured output (optional)
    pub output_schema: Option<serde_json::Value>,

    /// Maps output fields to state fields
    pub outputs: Option<HashMap<String, String>>,

    #[serde(default)]
    pub wait_for: WaitMode,
}
```

### Benefits

| Aspect | Without Schema | With Schema |
|--------|---------------|-------------|
| **Reliability** | LLM may ignore format | API enforces format |
| **Parsing** | Need fallback heuristics | Direct JSON parse |
| **Validation** | Runtime type errors | Schema-validated |
| **IDE Support** | None | Schema autocomplete |
| **Error Messages** | Cryptic parse errors | Clear schema violations |

### Fallback Strategy

When `output_schema` is not specified, use fallback parsing:

```rust
fn extract_output(raw: &str) -> Result<Value> {
    // 1. Try direct JSON parse
    if let Ok(json) = serde_json::from_str(raw) {
        return Ok(json);
    }

    // 2. Try extracting JSON from markdown code block
    if let Some(json_str) = extract_json_block(raw) {
        if let Ok(json) = serde_json::from_str(&json_str) {
            return Ok(json);
        }
    }

    // 3. Fallback: wrap as string
    Ok(json!({ "raw_output": raw }))
}

fn extract_json_block(text: &str) -> Option<String> {
    // Match ```json ... ``` blocks
    let re = regex::Regex::new(r"```(?:json)?\s*\n([\s\S]*?)\n```").unwrap();
    re.captures(text).map(|c| c[1].to_string())
}
```

### Complete Example

```yaml
kind: Graph
name: StructuredPRReview

workflow:
  state:
    pr_type: { type: string }
    risk_level: { type: string }
    issues: { type: array, reducer: append }

  nodes:
    - id: analyze
      agent:
        name: PRAnalyzer
        instructions: "Analyze the PR and classify it"
        model: { kind: llm }
        tools: [get_pull_request]
      output_schema:
        type: object
        properties:
          pr_type:
            type: string
            enum: [feature, bugfix, refactor, docs, test]
          risk_level:
            type: string
            enum: [low, medium, high, critical]
          summary:
            type: string
        required: [pr_type, risk_level, summary]
      outputs:
        pr_type: "pr_type"
        risk_level: "risk_level"

    - id: security_check
      depends_on: analyze
      when: "risk_level == 'high' or risk_level == 'critical'"
      agent:
        name: SecurityReviewer
        executor: react
        model: { kind: llm }
        tools: [scan_vulnerabilities]
      output_schema:
        type: object
        properties:
          vulnerabilities:
            type: array
            items:
              type: object
              properties:
                severity: { type: string }
                description: { type: string }
                location: { type: string }
          recommendation:
            type: string
        required: [vulnerabilities, recommendation]
      outputs:
        issues: "vulnerabilities"
```

---

## Condition Evaluation

### Syntax

Conditions are simple expressions evaluated against the workflow state.

```yaml
when: "<expression>"
```

### Supported Operators

| Category | Operators | Example |
|----------|-----------|---------|
| Equality | `==`, `!=` | `intent == 'search'` |
| Comparison | `>`, `>=`, `<`, `<=` | `confidence > 0.8` |
| Contains | `contains` | `tags contains 'bug'` |
| Null check | `== null`, `!= null` | `error == null` |
| Boolean | `== true`, `== false` | `is_draft == false` |
| Logical | `and`, `or` | `type == 'bug' and priority > 3` |

### Examples

```yaml
# Simple equality
when: "intent == 'search'"

# Numeric comparison
when: "confidence >= 0.8"

# Compound condition
when: "intent == 'code' and has_tests == false"

# Contains check
when: "file_types contains '.rs'"

# Null safety
when: "error == null and result != null"
```

### Evaluation Context

Conditions have access to:

```
state.<field>    → Declared state fields
input            → Original workflow input (string)
```

---

## Execution Model

### Graph Execution Algorithm

```
1. Initialize state from schema defaults
2. Set state.input = workflow input
3. Find entry nodes (no depends_on)
4. While there are pending nodes:
   a. Find ready nodes:
      - All dependencies completed
      - `when` condition satisfied (or no condition)
   b. Execute ready nodes in parallel
   c. For each completed node:
      - Parse output as JSON
      - Map outputs to state using reducers
      - Mark node as completed
5. Return final state as JSON
```

### Dependency Resolution

```yaml
nodes:
  - id: A                    # Entry node (no deps)
  - id: B                    # Entry node (no deps)
  - id: C, depends_on: A     # Waits for A
  - id: D, depends_on: [A,B] # Waits for A AND B
  - id: E, depends_on: C     # Waits for C
```

```
Execution order:
  Round 1: [A, B]     (parallel)
  Round 2: [C, D]     (parallel, after A,B complete)
  Round 3: [E]        (after C completes)
```

### Wait Modes

```yaml
# Default: wait for ALL dependencies
- id: final
  depends_on: [a, b, c]
  wait_for: all           # Default

# Wait for ANY dependency
- id: fallback
  depends_on: [primary, backup]
  wait_for: any           # Runs when first completes
```

---

## Rust Types

### Loader Types (loader.rs)

```rust
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GraphWorkflowDefinition {
    pub state: Option<StateSchema>,
    pub nodes: Vec<NodeDefinition>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StateSchema {
    #[serde(flatten)]
    pub fields: HashMap<String, StateFieldDef>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StateFieldDef {
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub reducer: Reducer,
    pub default: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(rename_all = "lowercase")]
pub enum Reducer {
    #[default]
    Overwrite,
    Append,
    Max,
    Min,
    Merge,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct NodeDefinition {
    pub id: String,
    pub agent: AgentConfig,
    #[serde(default)]
    pub depends_on: DependsOn,
    pub when: Option<String>,
    pub outputs: Option<HashMap<String, String>>,
    #[serde(default)]
    pub wait_for: WaitMode,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(untagged)]
pub enum DependsOn {
    #[default]
    None,
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(rename_all = "lowercase")]
pub enum WaitMode {
    #[default]
    All,
    Any,
}
```

### State Types (state.rs)

```rust
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct WorkflowState {
    fields: HashMap<String, Value>,
    reducers: HashMap<String, Reducer>,
}

impl WorkflowState {
    pub fn new(schema: &StateSchema) -> Self {
        let mut fields = HashMap::new();
        let mut reducers = HashMap::new();

        for (name, def) in &schema.fields {
            if let Some(default) = &def.default {
                fields.insert(name.clone(), default.clone());
            }
            reducers.insert(name.clone(), def.reducer.clone());
        }

        Self { fields, reducers }
    }

    pub fn update(&mut self, key: &str, value: Value) {
        let reducer = self.reducers.get(key).unwrap_or(&Reducer::Overwrite);

        match reducer {
            Reducer::Overwrite => {
                self.fields.insert(key.to_string(), value);
            }
            Reducer::Append => {
                let arr = self.fields
                    .entry(key.to_string())
                    .or_insert(Value::Array(vec![]));
                if let Value::Array(a) = arr {
                    if let Value::Array(new_items) = value {
                        a.extend(new_items);
                    } else {
                        a.push(value);
                    }
                }
            }
            Reducer::Max => {
                let current = self.fields.get(key)
                    .and_then(|v| v.as_f64())
                    .unwrap_or(f64::MIN);
                if let Some(new) = value.as_f64() {
                    if new > current {
                        self.fields.insert(key.to_string(), value);
                    }
                }
            }
            // ... other reducers
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.fields.get(key)
    }

    pub fn to_json(&self) -> Value {
        Value::Object(self.fields.clone().into_iter().collect())
    }
}
```

### Graph Agent (graph.rs)

```rust
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub struct GraphAgent {
    pub name: String,
    pub nodes: HashMap<String, CompiledNode>,
    pub state_schema: StateSchema,
    pub node_order: Vec<String>,  // Topological order
}

struct CompiledNode {
    id: String,
    agent: Arc<dyn Agent>,
    depends_on: Vec<String>,
    when: Option<CompiledCondition>,
    outputs: HashMap<String, String>,
    wait_mode: WaitMode,
}

#[async_trait]
impl Agent for GraphAgent {
    fn name(&self) -> String {
        self.name.clone()
    }

    async fn run(&self, input: String) -> Result<String, Error> {
        let mut state = WorkflowState::new(&self.state_schema);
        state.update("input", Value::String(input));

        let mut completed: HashSet<String> = HashSet::new();

        loop {
            // Find nodes ready to execute
            let ready: Vec<&CompiledNode> = self.nodes.values()
                .filter(|n| !completed.contains(&n.id))
                .filter(|n| self.dependencies_satisfied(n, &completed))
                .filter(|n| self.condition_met(n, &state))
                .collect();

            if ready.is_empty() {
                break;
            }

            // Execute ready nodes in parallel
            let results = futures::future::join_all(
                ready.iter().map(|n| self.execute_node(n, &state))
            ).await;

            // Process results
            for (node, result) in ready.iter().zip(results) {
                match result {
                    Ok(output) => {
                        self.apply_outputs(node, &output, &mut state);
                        completed.insert(node.id.clone());
                    }
                    Err(e) => {
                        log::error!("Node {} failed: {}", node.id, e);
                        state.update(&format!("{}.error", node.id),
                                    Value::String(e.to_string()));
                    }
                }
            }
        }

        Ok(state.to_json().to_string())
    }

    fn dependencies_satisfied(&self, node: &CompiledNode, completed: &HashSet<String>) -> bool {
        match node.wait_mode {
            WaitMode::All => node.depends_on.iter().all(|d| completed.contains(d)),
            WaitMode::Any => node.depends_on.is_empty() ||
                             node.depends_on.iter().any(|d| completed.contains(d)),
        }
    }

    fn condition_met(&self, node: &CompiledNode, state: &WorkflowState) -> bool {
        match &node.when {
            Some(condition) => condition.evaluate(state),
            None => true,
        }
    }
}
```

### Condition Evaluator (condition.rs)

```rust
pub struct CompiledCondition {
    expression: Expression,
}

enum Expression {
    Compare {
        left: ValuePath,
        op: CompareOp,
        right: LiteralValue,
    },
    And(Box<Expression>, Box<Expression>),
    Or(Box<Expression>, Box<Expression>),
}

enum CompareOp {
    Eq, NotEq, Gt, Gte, Lt, Lte, Contains,
}

impl CompiledCondition {
    pub fn parse(expr: &str) -> Result<Self, Error> {
        // Parse "field == 'value'" into Expression
    }

    pub fn evaluate(&self, state: &WorkflowState) -> bool {
        self.expression.eval(state)
    }
}
```

---

## Module Structure

### Directory Layout

```
src/
├── adk/
│   ├── mod.rs
│   ├── agent.rs           # Agent trait, LLMAgent, ReActAgent (existing)
│   ├── model.rs           # Model trait, Content, Part (existing)
│   ├── gemini.rs          # Gemini implementation (existing)
│   └── tool.rs            # Tool trait (existing)
│
├── kinetic/
│   ├── mod.rs
│   ├── tools/             # Tool implementations (existing)
│   ├── mcp/               # MCP integration (existing)
│   │
│   └── workflow/
│       ├── mod.rs         # Re-exports public API
│       │
│       │── # ─── EXISTING ───
│       ├── loader.rs      # YAML parsing, WorkflowDefinition
│       ├── builder.rs     # Builds agents from definitions
│       ├── registry.rs    # Tool registry
│       │
│       │── # ─── NEW: GRAPH ───
│       ├── graph/
│       │   ├── mod.rs     # Re-exports graph module
│       │   ├── types.rs   # GraphWorkflowDef, NodeDef, EdgeDef
│       │   ├── executor.rs # GraphAgent implementation
│       │   ├── scheduler.rs # Dependency resolution, ready nodes
│       │   └── normalizer.rs # Direct/Composite → Graph conversion
│       │
│       │── # ─── NEW: STATE ───
│       ├── state/
│       │   ├── mod.rs     # Re-exports state module
│       │   ├── schema.rs  # StateSchema, StateFieldDef
│       │   ├── store.rs   # WorkflowState storage
│       │   ├── reducer.rs # Reducer implementations
│       │   └── extractor.rs # JSON path extraction from outputs
│       │
│       │── # ─── NEW: CONDITIONS ───
│       └── condition/
│           ├── mod.rs     # Re-exports condition module
│           ├── parser.rs  # Expression parser
│           ├── ast.rs     # Expression AST types
│           └── evaluator.rs # Condition evaluation

schemas/
├── workflow.schema.json   # Extended with graph schema
└── graph.schema.json      # Optional: separate graph schema
```

### Module Responsibilities

#### `workflow/graph/` - Graph Execution Engine

| File | Responsibility |
|------|----------------|
| `types.rs` | Data structures: `GraphWorkflowDef`, `NodeDefinition`, `DependsOn`, `WaitMode` |
| `executor.rs` | `GraphAgent` implementation, main execution loop |
| `scheduler.rs` | `Scheduler` - determines ready nodes, topological sort, cycle detection |
| `normalizer.rs` | `Normalizer` - converts `Direct`/`Composite` to graph representation |

#### `workflow/state/` - State Management

| File | Responsibility |
|------|----------------|
| `schema.rs` | `StateSchema`, `StateFieldDef` - state type definitions |
| `store.rs` | `WorkflowState` - runtime state storage and access |
| `reducer.rs` | `Reducer` enum and implementations (overwrite, append, max, min, merge) |
| `extractor.rs` | `OutputExtractor` - extracts values from agent output using paths |

#### `workflow/condition/` - Condition Evaluation

| File | Responsibility |
|------|----------------|
| `ast.rs` | `Expression`, `CompareOp`, `ValuePath`, `Literal` - AST types |
| `parser.rs` | `ConditionParser` - parses `"field == 'value'"` into AST |
| `evaluator.rs` | `ConditionEvaluator` - evaluates AST against state |

### Trait Abstractions

```rust
// ─── workflow/graph/mod.rs ───

/// Trait for scheduling node execution
pub trait NodeScheduler: Send + Sync {
    /// Get nodes ready to execute
    fn get_ready_nodes(
        &self,
        completed: &HashSet<String>,
        state: &WorkflowState,
    ) -> Vec<String>;

    /// Check if all nodes are complete
    fn is_complete(&self, completed: &HashSet<String>) -> bool;
}

/// Trait for normalizing workflow definitions to graph
pub trait WorkflowNormalizer {
    fn normalize(&self, def: &WorkflowDefinition) -> Result<GraphWorkflowDef, Error>;
}
```

```rust
// ─── workflow/state/mod.rs ───

/// Trait for reducing values into state
pub trait StateReducer: Send + Sync {
    fn reduce(&self, current: Option<&Value>, new: Value) -> Value;
}

/// Trait for extracting values from agent output
pub trait OutputExtractor: Send + Sync {
    fn extract(&self, output: &Value, path: &str) -> Option<Value>;
}
```

```rust
// ─── workflow/condition/mod.rs ───

/// Trait for parsing conditions
pub trait ConditionParser: Send + Sync {
    fn parse(&self, expr: &str) -> Result<Box<dyn Condition>, Error>;
}

/// Trait for evaluatable conditions
pub trait Condition: Send + Sync {
    fn evaluate(&self, state: &WorkflowState) -> bool;
}
```

### Key Types by Module

```rust
// ─── workflow/graph/types.rs ───

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GraphWorkflowDef {
    pub state: Option<StateSchema>,
    pub nodes: Vec<NodeDefinition>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodeDefinition {
    pub id: String,
    pub agent: AgentConfig,
    #[serde(default)]
    pub depends_on: DependsOn,
    pub when: Option<String>,
    /// JSON Schema for structured output (enforced by LLM API)
    pub output_schema: Option<serde_json::Value>,
    /// Maps output fields to state fields
    pub outputs: Option<HashMap<String, String>>,
    #[serde(default)]
    pub wait_for: WaitMode,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(untagged)]
pub enum DependsOn {
    #[default]
    None,
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WaitMode {
    #[default]
    All,
    Any,
}
```

```rust
// ─── workflow/state/schema.rs ───

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StateSchema {
    #[serde(flatten)]
    pub fields: HashMap<String, StateFieldDef>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StateFieldDef {
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(default)]
    pub reducer: ReducerType,
    pub default: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    String,
    Number,
    Boolean,
    Array,
    Object,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ReducerType {
    #[default]
    Overwrite,
    Append,
    Max,
    Min,
    Merge,
}
```

```rust
// ─── workflow/condition/ast.rs ───

#[derive(Debug, Clone)]
pub enum Expression {
    Compare {
        left: ValuePath,
        op: CompareOp,
        right: Literal,
    },
    And(Box<Expression>, Box<Expression>),
    Or(Box<Expression>, Box<Expression>),
    Not(Box<Expression>),
}

#[derive(Debug, Clone)]
pub struct ValuePath(pub Vec<String>);  // e.g., ["state", "intent"]

#[derive(Debug, Clone, PartialEq)]
pub enum CompareOp {
    Eq,
    NotEq,
    Gt,
    Gte,
    Lt,
    Lte,
    Contains,
}

#[derive(Debug, Clone)]
pub enum Literal {
    String(String),
    Number(f64),
    Boolean(bool),
    Null,
}
```

### Public API (mod.rs exports)

```rust
// ─── workflow/mod.rs ───

// Existing exports
pub mod loader;
pub mod builder;
pub mod registry;

// New graph workflow exports
pub mod graph;
pub mod state;
pub mod condition;

// Re-export commonly used types
pub use graph::{GraphWorkflowDef, NodeDefinition, GraphAgent};
pub use state::{WorkflowState, StateSchema};
pub use condition::Condition;
```

```rust
// ─── workflow/graph/mod.rs ───

mod types;
mod executor;
mod scheduler;
mod normalizer;

pub use types::*;
pub use executor::GraphAgent;
pub use scheduler::DefaultScheduler;
pub use normalizer::DefaultNormalizer;
```

### Builder Integration

```rust
// ─── workflow/builder.rs (updated) ───

use crate::kinetic::workflow::graph::{GraphAgent, DefaultNormalizer, DefaultScheduler};
use crate::kinetic::workflow::state::WorkflowState;

impl Builder {
    pub async fn build_from_def(&self, def: &WorkflowDefinition) -> Result<Arc<dyn Agent>> {
        // Normalize all workflow types to graph
        let normalizer = DefaultNormalizer::new();
        let graph_def = normalizer.normalize(def)?;

        // Build graph agent
        self.build_graph_agent(&graph_def).await
    }

    async fn build_graph_agent(&self, def: &GraphWorkflowDef) -> Result<Arc<dyn Agent>> {
        // Build individual node agents
        let mut nodes = HashMap::new();
        for node_def in &def.nodes {
            let agent = self.build_node_agent(&node_def.agent).await?;
            nodes.insert(node_def.id.clone(), CompiledNode {
                id: node_def.id.clone(),
                agent,
                depends_on: node_def.depends_on.to_vec(),
                when: node_def.when.as_ref().map(|w| ConditionParser::parse(w)).transpose()?,
                outputs: node_def.outputs.clone().unwrap_or_default(),
                wait_mode: node_def.wait_for.clone(),
            });
        }

        let scheduler = DefaultScheduler::new(&nodes);
        let state_schema = def.state.clone().unwrap_or_default();

        Ok(Arc::new(GraphAgent::new(
            def.name.clone(),
            nodes,
            scheduler,
            state_schema,
        )))
    }
}
```

### Testing Structure

```
tests/
├── workflow/
│   ├── graph_test.rs      # GraphAgent execution tests
│   ├── scheduler_test.rs  # Dependency resolution tests
│   ├── normalizer_test.rs # Workflow normalization tests
│   ├── state_test.rs      # State and reducer tests
│   └── condition_test.rs  # Condition parsing/evaluation tests
│
└── integration/
    ├── graph_workflow_test.rs  # End-to-end graph workflows
    └── mixed_executor_test.rs  # Graph + ReAct integration
```

---

## Migration Path

### Phase 1: Core Types
1. Add new types to `loader.rs`
2. Implement `WorkflowState` in `state.rs`
3. Add condition evaluator in `condition.rs`

### Phase 2: Graph Executor
1. Implement `GraphAgent` in `graph.rs`
2. Add `build_graph_agent()` to builder
3. Add `kind: Graph` support

### Phase 3: Normalization
1. Add `to_graph()` normalization for `Direct`
2. Add `to_graph()` normalization for `Composite`
3. Route all execution through `GraphAgent`

### Phase 4: Testing & Examples
1. Unit tests for state, conditions, graph execution
2. Example workflows demonstrating patterns
3. Update documentation

---

## Example Workflows

### Intent Router

```yaml
kind: Graph
name: IntentRouter

workflow:
  state:
    intent: { type: string }
    response: { type: string }

  nodes:
    - id: classify
      agent:
        name: Classifier
        instructions: |
          Classify the query. Output: {"intent": "search|code|chat"}
        model: { kind: llm }
        tools: []
      outputs:
        intent: "intent"

    - id: search
      depends_on: classify
      when: "intent == 'search'"
      agent: { file: agents/search.yaml }
      outputs:
        response: "result"

    - id: code
      depends_on: classify
      when: "intent == 'code'"
      agent: { file: agents/code.yaml }
      outputs:
        response: "result"

    - id: chat
      depends_on: classify
      when: "intent == 'chat'"
      agent: { file: agents/chat.yaml }
      outputs:
        response: "result"
```

### Parallel Aggregation

```yaml
kind: Graph
name: MultiSourceResearch

workflow:
  state:
    findings: { type: array, reducer: append }
    summary: { type: string }

  nodes:
    - id: search_web
      agent: { file: agents/web_search.yaml }
      outputs:
        findings: "results"

    - id: search_docs
      agent: { file: agents/doc_search.yaml }
      outputs:
        findings: "results"

    - id: search_code
      agent: { file: agents/code_search.yaml }
      outputs:
        findings: "results"

    - id: summarize
      depends_on: [search_web, search_docs, search_code]
      agent:
        name: Summarizer
        instructions: "Synthesize all findings into a coherent summary"
        model: { kind: llm }
        tools: []
      outputs:
        summary: "summary"
```

---

## ReAct Integration

ReAct (Reasoning + Acting) and Graph workflows are **orthogonal concepts** that compose naturally.

### Two Levels of Execution

| Level | Concept | Controls |
|-------|---------|----------|
| **Workflow** | Graph | Which agents run, in what order, with what conditions |
| **Agent** | ReAct | How a single agent reasons and uses tools internally |

```
┌─────────────────────────────────────────────────────────────┐
│                    GRAPH WORKFLOW                            │
│  ┌─────────┐     ┌─────────────────┐     ┌─────────┐        │
│  │ Node A  │────▶│     Node B      │────▶│ Node C  │        │
│  │         │     │ executor: react │     │         │        │
│  └─────────┘     └─────────────────┘     └─────────┘        │
│                          │                                   │
│                          ▼                                   │
│              ┌───────────────────────┐                       │
│              │   ReAct Loop          │                       │
│              │  Thought → Action     │                       │
│              │     → Observation     │                       │
│              │     → Thought → ...   │                       │
│              └───────────────────────┘                       │
└─────────────────────────────────────────────────────────────┘
```

### Example: Graph with ReAct Nodes

```yaml
kind: Graph
name: SmartPRReview

workflow:
  state:
    pr_info: { type: object }
    security_findings: { type: array, reducer: append }
    code_review: { type: string }

  nodes:
    # Simple agent - default executor
    - id: fetch_pr
      agent:
        name: PRFetcher
        instructions: "Fetch PR metadata"
        model: { kind: llm }
        tools: [get_pull_request, get_pull_request_diff]
      outputs:
        pr_info: "pr"

    # ReAct agent - complex reasoning with tools
    - id: security_review
      depends_on: fetch_pr
      when: "pr_info.has_security_files == true"
      agent:
        name: SecurityReviewer
        executor: react              # ← ReAct execution!
        max_iterations: 8
        instructions: |
          You are a security expert. Analyze the PR for vulnerabilities.
          Use tools to search for known CVEs and security patterns.
        model: { kind: llm }
        tools:
          - search_cve_database
          - check_dependency_vulnerabilities
      outputs:
        security_findings: "findings"

    # Another ReAct agent for deep analysis
    - id: deep_code_review
      depends_on: fetch_pr
      agent:
        name: CodeReviewer
        executor: react              # ← ReAct execution!
        max_iterations: 10
        instructions: |
          Perform thorough code review. Check for:
          - Code quality and best practices
          - Test coverage gaps
          - Performance issues
        model: { kind: llm }
        tools:
          - analyze_complexity
          - check_test_coverage
      outputs:
        code_review: "review"

    # Simple summarizer - no tools needed, default executor
    - id: summarize
      depends_on: [security_review, deep_code_review]
      agent:
        name: Summarizer
        instructions: "Combine all findings into final review"
        model: { kind: llm }
        tools: []
```

### Execution Flow

**Graph Level:**
```
1. Start with fetch_pr (entry node)
2. When fetch_pr completes:
   - If has_security_files == true: run security_review
   - Always: run deep_code_review
3. When both complete: run summarize
4. Return final state
```

**Agent Level (ReAct at security_review node):**
```
Iteration 1:
  Thought: "Need to check dependencies for vulnerabilities"
  Action: check_dependency_vulnerabilities({"files": [...]})
  Observation: "Found 2 vulnerable packages: lodash@4.17.15, axios@0.21.0"

Iteration 2:
  Thought: "Should check for known CVEs related to these"
  Action: search_cve_database({"packages": ["lodash", "axios"]})
  Observation: "CVE-2021-23337 affects lodash, CVE-2021-3749 affects axios"

Iteration 3:
  Thought: "Have enough information to report findings"
  Final Answer: {
    "findings": [
      {"severity": "high", "cve": "CVE-2021-23337", "package": "lodash"},
      {"severity": "medium", "cve": "CVE-2021-3749", "package": "axios"}
    ]
  }
```

### State Flow with ReAct

ReAct agents output to state just like regular agents. The **Final Answer** is parsed and mapped:

```yaml
- id: researcher
  agent:
    executor: react
    instructions: "Research the topic thoroughly"
    tools: [search, analyze]
  outputs:
    findings: "findings"      # Maps Final Answer's "findings" to state
    confidence: "confidence"
    sources: "sources"
```

**ReAct Final Answer must be JSON for output mapping:**
```
Final Answer: {
  "findings": ["fact1", "fact2"],
  "confidence": 0.85,
  "sources": ["https://example.com"]
}
```

### Executor Compatibility

| Executor | Tools | Use Case |
|----------|-------|----------|
| `default` | Yes/No | Simple tasks, single-turn |
| `react` | Yes | Complex reasoning, multi-step |
| `cot` | No | Chain-of-thought prompting |

Any node can use any executor:

```yaml
nodes:
  - id: simple_task
    agent:
      executor: default       # Quick, single response

  - id: complex_research
    agent:
      executor: react         # Multi-step reasoning
      max_iterations: 10

  - id: reasoning_task
    agent:
      executor: cot           # Step-by-step thinking
```

### Combined Patterns

| Pattern | Description |
|---------|-------------|
| **Graph + Default** | Simple linear/parallel workflows |
| **Graph + ReAct** | Complex reasoning at specific nodes |
| **Graph + Mixed** | Right executor for each task |

---

## Open Questions

1. **Error Handling**: How should node failures affect dependent nodes?
   - Option A: Mark as failed, skip dependents
   - Option B: Expose error in state, let conditions decide

2. **Timeout**: Should nodes have individual timeouts?

3. **Retry**: Should failed nodes be retried automatically?

4. **Subgraphs**: Should we support nested graph workflows?

---

## References

- [LangGraph Documentation](https://langchain-ai.github.io/langgraph/)
- [AutoAgents Framework](https://github.com/liquidos-ai/AutoAgents)
- [rs-graph-llm](https://github.com/a-agmon/rs-graph-llm)

