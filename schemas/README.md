# Kinetic-rs Schemas

JSON Schema definitions for validating workflow and agent YAML files.

## Usage

### VS Code / Cursor (Recommended)

The `.vscode/settings.json` is configured to automatically apply the schema to all YAML files in `agents/` and `examples/` directories.

**Requirements:**
- Install the [YAML extension](https://marketplace.visualstudio.com/items?itemName=redhat.vscode-yaml) by Red Hat

Once installed, you'll get:
- ✅ Autocomplete for all fields
- ✅ Validation errors for invalid configurations
- ✅ Hover documentation

### Manual Schema Reference

Add this comment to the top of any YAML file:

```yaml
# yaml-language-server: $schema=../schemas/workflow.schema.json
```

Adjust the path based on your file's location.

## Schema Overview

### Workflow Types

| Kind | Description |
|------|-------------|
| `Direct` | Single agent workflow - requires `agent` field |
| `Composite` | Multi-agent workflow - requires `workflow` field |

### Execution Modes (Composite)

| Mode | Description |
|------|-------------|
| `sequential` | Agents run one after another, output flows to next |
| `parallel` | Agents run concurrently, outputs combined |
| `loop` | Agents repeat until max_iterations or termination condition |

### Model Providers

| Provider | Description |
|----------|-------------|
| `Gemini` | Google Gemini API |
| `OpenAI` | OpenAI API |
| `Anthropic` | Anthropic Claude API |

## Examples

### Direct Agent

```yaml
kind: Direct
name: MyAgent
description: "A simple agent"

agent:
  name: MyAgent
  description: "Does something useful"
  instructions: |
    You are a helpful assistant.
  model:
    kind: llm
    provider: Gemini
  tools:
    - tool_name
```

### Composite Workflow

```yaml
kind: Composite
name: MyWorkflow
description: "Multi-step workflow"

workflow:
  execution: sequential
  agents:
    - file: agents/step1.yaml
    - file: agents/step2.yaml
```

