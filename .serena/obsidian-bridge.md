# Serena ↔ Obsidian Bridge

This file documents how Serena (code intelligence) and Obsidian (knowledge management) work together in this project.

## Architecture
- **Obsidian vault root** = project root (`C:\dev\AetherDash`)
- **Serena project root** = same directory
- **Serena memories** → `.serena/memories/` (markdown files, also visible in Obsidian)
- **Blueprint docs** → `aether-blueprint/` (visible in both tools)
- **Generated vault** → `vault/` (Obsidian-compatible markdown, generated from DB — never hand-edit)

## Workflow
1. **Code work:** Serena provides code intelligence (find_symbol, find_referencing_symbols, etc.)
2. **Knowledge work:** Obsidian provides graph view, backlinks, canvas for blueprint docs
3. **Memory:** Serena memories capture durable project knowledge; visible in Obsidian for browsing
4. **Generation:** Brain service generates `vault/` from DB; Obsidian reads it for knowledge visualization

## Obsidian Features Active
- Graph view: see relationships between blueprint docs, specs, and ExecPlans
- Backlinks: trace where each ADR/spec/ExecPlan is referenced
- Canvas: visual architecture diagrams
- Tags: cross-reference specs, plans, and ADRs
- Daily notes: development journal

## Serena Features Active
- Symbol-level code reading (token-efficient)
- Find references across the codebase
- Project memories for durable knowledge
- Semantic editing (replace_symbol_body, insert_after_symbol)

## Key Files (both tools)
| File | Obsidian Use | Serena Use |
|------|-------------|------------|
| `aether-blueprint/ARCHITECTURE.md` | Knowledge graph node | Reference for code architecture |
| `aether-blueprint/AGENTS.md` | Agent governance doc | Coding agent rules |
| `aether-blueprint/DECISIONS.md` | Decision log | ADR reference |
| `.agent/execplans/*.md` | Active work tracking | Execution context |
