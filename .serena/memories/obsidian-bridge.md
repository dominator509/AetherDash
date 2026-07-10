# Serena ↔ Obsidian Cohesion

This project uses both Serena (code intelligence) and Obsidian (knowledge management) at the same root directory. They complement each other:

## How They Work Together
- **Obsidian vault root** = project root (`.obsidian/` config present)
- **Serena project root** = same directory (`.serena/` config present)
- Both tools index the same markdown files — no duplication
- Serena memories (`.serena/memories/`) are markdown files visible in Obsidian's file explorer
- `aether-blueprint/` docs appear in Obsidian's graph view with backlinks between them

## Bridge File
See `.serena/obsidian-bridge.md` for the detailed cross-tool reference.

## Workflow Division
- **Code reading/editing:** Use Serena tools (find_symbol, get_symbols_overview, replace_symbol_body)
- **Project knowledge browsing:** Use Obsidian (graph view, backlinks, canvas)
- **Durable knowledge:** Write to Serena memories; they're also markdown visible in Obsidian
- **Architecture visualization:** Obsidian canvas from `ARCHITECTURE.md` + graph view
- **Execution tracking:** Obsidian sees `.agent/execplans/*.md` with backlink support

## Generated Vault (`vault/`)
- One-way: DB → Markdown (never hand-edited)
- Obsidian-compatible format
- Brain service regenerates on schedule
- Visible in Obsidian for knowledge exploration but read-only for agents

## Key Obsidian Features Enabled
- Graph view: cross-reference blueprint docs, specs, ExecPlans
- Backlinks: trace ADR/spec/ExecPlan references
- Canvas: visual diagrams
- Daily notes: development journal
- Templates: `.obsidian/templates/` for consistent note creation