Layer: 5 - Execution

# EP-204: Agentic Inbox

**Band:** 2xx Brain | **Phase:** 1 | **Status:** done | **Blocked by:** EP-201

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
- [x] M1 Webhooks  - [x] M2 Fetch+dedup  - [x] M3 Safe parsing  - [x] M4 Filing  - [x] M5 Reprocess

## Surprises & Discoveries
Gmail Pub/Sub sends an authenticated OIDC bearer token and a base64 JSON data envelope; it does not send the originally assumed snake-case fields. Graph clientState is carried on each notification entry. Thread cancellation cannot stop a hostile parser, so parsing now runs under a killed-on-timeout isolated subprocess with Windows Job Object or POSIX rlimits plus Python audit-hook capability denial.

## Decision Log
Provider variables are finalized in ENVIRONMENT.md. Reprocess uses the shared session/grant authenticator, invalidates derived artifacts, and reruns the existing object rather than trusting X-Tier or creating a duplicate. Webhooks commit to a SQLite WAL queue before acknowledgment; workers use expiring leases and bounded retry backoff. Gmail cursors advance only after successful filing. Parser children receive no credentials, run in isolated mode, deny network/subprocess/dynamic-load/write capabilities, and have CPU, memory, output, input, and wall-time ceilings.

## Outcomes & Retrospective
Authenticated webhook parsing, durable queue/cursor recovery, real provider API adapters, atomic content dedup, resource-isolated parsing, original-byte preservation, low-trust Brain filing, and authenticated in-place reprocessing are implemented. Provider-stub integration proves webhook -> durable queue -> fetch -> sandbox parse -> Brain.Store with origin=inbox and trust=low. Validation: 47 inbox tests, 6 Brain storage tests, Ruff, MyPy (24 source files), and git diff --check all pass. Real provider credentials remain operator-owned STOP S1 and were not used.
