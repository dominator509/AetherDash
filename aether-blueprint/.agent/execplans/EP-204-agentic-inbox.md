Layer: 5 - Execution

# EP-204: Agentic Inbox

**Band:** 2xx Brain | **Phase:** 1 | **Status:** draft | **Blocked by:** EP-201

## Purpose / Big Picture
Let the operator feed the Brain by forwarding email and documents: a webhook receiver for Gmail push / MS Graph that safely parses messages and attachments and files them as provenance-carrying Brain objects with `origin=inbox, trust=low`. This is the Brain's first real ingestion source.

## Scope
`server/inbox` service: Gmail push + MS Graph webhook receivers, message/attachment fetch, resource-limited safe parsing (PDF/text/image-as-screenshot), dispatch into the EP-201 pipeline as ObjectDrafts, dedup, low-trust flagging, inbox reprocess tool.

## Non-goals
No OCR quality work (EP-206 owns OCR depth; here images enter as `screenshot` kind for later OCR), no ingestion fleet/web sources (EP-206), no source-reliability scoring (EP-206), no outbound email (EP-308).

## Context and Orientation
SECURITY.md inbox boundary is binding: attachments parse in a resource-limited worker with NO macro execution and NO external fetches during parse; extracted content enters `trust=low` until EP-206 scoring. INV-3/INV-1: inbox content is DATA, never instructions (prompt-injection defense, SECURITY.md T3). Objects get full provenance (SPEC-011).

## Files to Read First
1. SPEC-011 (object model, pipeline entry, trust); SECURITY.md (inbox boundary, T3 prompt injection).
2. EP-201 pipeline `Brain.Store` entry + ObjectDraft shape; ENVIRONMENT.md inbox rows (finalize names here).

## Files to Change (Expected Changed Files)
`server/inbox/**` (app, webhooks/{gmail,msgraph}.py, fetch.py, parse/{pdf,text,image}.py, filing.py), webhook receiver bind (`AETHER_INBOX__BIND`), ENVIRONMENT.md `AETHER_INBOX__GMAIL_*`/`__MSGRAPH_*` finalized rows, uv workspace member, COMMANDS.md inbox start line (present), CHANGELOG, this file.

## Interfaces and Contracts
Webhook receivers validate provider signatures/tokens; fetched content -> ObjectDraft{kind: email|document|screenshot, origin: inbox, trust: low, source: from-address, raw bytes} -> `Brain.Store` (EP-201 pipeline). Parsing is sandboxed (no network, resource caps, no macro/exec). Reprocess = tier-3 action (SPEC-005) re-running the pipeline on stored raw.

## Milestones
1. **Webhook receivers.** Gmail push (Pub/Sub-style) + MS Graph subscription receivers with signature/token validation; malformed/unauthenticated -> reject + metric, no content reflected in logs. Done when: integration against provider webhook stubs asserts valid accepted / invalid rejected; no content in logs (redaction test).
2. **Fetch + dedup.** Pull message + attachments via provider API (stubbed), content-hash dedup (a re-forwarded mail short-circuits). Done when: fetch integration (stub) + dedup test.
3. **Safe parsing.** PDF text extraction, plain text, images stored as `screenshot` kind (bytes to MinIO for later OCR); parser runs resource-limited, no external fetch, no macro exec. Done when: parse tests incl. a hostile-fixture test (macro-laden doc / external-ref PDF -> parsed inertly, no fetch, no exec); resource-limit test.
4. **Filing to Brain.** ObjectDrafts -> pipeline; objects land `trust=low`, full provenance, correct kind; visible in recall only after index (EP-201 semantics). Done when: end-to-end integration: forwarded fixture email -> Brain object with provenance, recallable, low-trust flagged.
5. **Reprocess tool.** `inbox.reprocess` (tier-3) re-runs the pipeline on stored raw (e.g., after EP-206 OCR lands). Done when: reprocess integration test; tier-gating asserted.

## Concrete Steps
Provider auth via ENVIRONMENT.md names (finalize the exact var names in this plan + update ENVIRONMENT.md). Parsing sandbox: run parsers with resource limits and a no-network guarantee (subprocess with restricted env or in-process with hardened libs + a network-block assertion in tests). Treat every byte as hostile data (T3). Where EP-206 OCR is absent, screenshots park for OCR (documented). Commit per milestone.

## Validation and Acceptance
Per-milestone; `test-integration.sh` green (provider stubs, no real accounts - real credentials are STOP S1); `verify.sh` -> `verify: ok`; the hostile-fixture parse test is REQUIRED (no exec, no external fetch); low-trust flagging + provenance verified; `git diff --name-only` matches. Acceptance: SPEC-000 inbox requirement (forwarded content -> Brain objects with provenance) demonstrated.

## Idempotence and Recovery
Content-hash dedup makes re-forwarding and webhook retries safe; a crash mid-fetch resumes from the provider subscription cursor. Raw is preserved so reprocess is always possible. Real provider setup (OAuth, subscriptions) is operator work gated by S1.

## Progress
- [ ] M1 Webhooks  - [ ] M2 Fetch+dedup  - [ ] M3 Safe parsing  - [ ] M4 Filing  - [ ] M5 Reprocess

## Surprises & Discoveries
(provider webhook/subscription realities; parser hardening)

## Decision Log
(exact env var names; parsing sandbox mechanism)

## Outcomes & Retrospective
(inbox demonstrated; hostile-parse evidence; OCR seam for EP-206)
