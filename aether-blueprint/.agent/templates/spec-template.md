Layer: 4 - Specification

# SPEC-XXX: <Feature Name>

**Status:** draft | **Owning plans:** EP-XXX | **Last updated:** YYYY-MM-DD

## User-visible goal
What the operator/user can do when this behavior exists, in one paragraph.

## Non-goals
What this spec deliberately does not cover.

## Terms
Definitions used below. One line each. Reuse ARCHITECTURE.md vocabulary; do not coin synonyms.

## Required behavior
Numbered, testable statements ("MUST/MUST NOT"). Each maps to at least one required test.

## Inputs
Exact shapes: messages, payloads, user actions, config keys - with types and validation rules.

## Outputs
Exact shapes: responses, bus events, DB writes, UI states produced.

## Error states
Enumerated failures, their detection, user-visible result, retry/backoff semantics, audit requirements.

## Data rules
Persistence, provenance, staleness/expiry, tiering, redaction obligations for data this feature touches.

## API contracts (if applicable)
Proto/gRPC methods, WS message types, MCP tools - names, fields, versioning notes.

## UI states (if applicable)
Simple mode and Advanced mode both. Loading/empty/error/success. Keyboard access.

## Security rules
Permission tier required per action; hard-deny interactions; secret-handling constraints; trust-boundary validation.

## Accessibility rules (if applicable)
Keyboard path, semantic structure, no color-only signaling, plain-language summary requirement.

## Performance expectations (if applicable)
Budgets with numbers (latency, cadence, cache-hit) and where they are measured.

## Required tests
Named test cases (use test-case-template) per behavior: unit / integration / replay / e2e.

## Acceptance criteria
The short list that, when all pass, means this spec is implemented.
