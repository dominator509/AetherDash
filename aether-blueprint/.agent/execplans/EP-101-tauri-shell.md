Layer: 5 - Execution

# EP-101: Tauri Shell - Toggle, Keyboard Nav, Command Line, Encrypted Cache

**Band:** 1xx Client | **Phase:** 1 | **Status:** draft | **Blocked by:** EP-004

## Purpose / Big Picture
Stand up the client plane: a Tauri v2 desktop app whose Rust shell owns the keychain and the encrypted local cache, whose web layer connects to the gateway over the SPEC-003 WebSocket, and whose interaction model (Simple/Advanced toggle, palette, keyboard-first navigation) is in place before any surface has real content. Every later client plan (EP-102/103/104) mounts into this shell.

## Scope
Tauri v2 app (`client/`), Rust shell (`client/src-tauri`) with keychain + encrypted cache + gateway URL config, React/TS/Tailwind/Radix web layer, WS client with reconnect, session/login flow against the gateway auth stub, Simple/Advanced toggle, Ctrl/Cmd+K palette, global keyboard router, focus management, loading/empty/error primitives, theme (dark/light).

## Non-goals
No feed content (EP-102), no command room (EP-103), no advanced panels/DOM (EP-104), no real permission enforcement (client renders tiers only, SPEC-005), no auto-updater (EP-407).

## Context and Orientation
Read SPEC-004 in full - it is this plan's behavior contract. ADR-0008 fixes Tauri v2 and the shell-owns-secrets boundary; A-07 requires verifying Tauri v2 API details against the installed tauri-cli BEFORE writing shell code. The web layer NEVER touches secrets (SECURITY.md); keychain access is a Tauri command surface only.

## Files to Read First
1. SPEC-004 (entire); SPEC-003 gateway WS frame table + error envelope.
2. ADR-0008; SECURITY.md secret-handling; ENVIRONMENT.md client rows (`AETHER_CLIENT__GATEWAY_URL`).
3. frontend-design SKILL.md (before any UI code - environment styling constraints).

## Files to Change (Expected Changed Files)
`client/**` (src-tauri/{Cargo.toml,tauri.conf.json,src/{main.rs,keychain.rs,cache.rs,config.rs}}, src/{main.tsx,app.tsx, lib/ws.ts, lib/keyboard.ts, lib/session.ts, state/, components/{palette,toggle,shell,states}/, styles}), `client/package.json`, `client/e2e/` (Playwright setup + first specs), pnpm-workspace member append (client already listed per EP-001), root tsconfig ref, CHANGELOG, this file. Client is a pnpm package `@aether/client`; consumes `@aether/types`.

## Interfaces and Contracts
WS client implements every client->server and handles every server->client frame type from SPEC-003 (unknown server type -> visible error state, never crash). Session token stored via keychain command only; never in web storage (the artifact browser-storage ban also applies - React state only). Mode is session-scoped UI state synced to the gateway on connect (a `subscribe`-adjacent preference; if the gateway stub doesn't persist it yet, keep client-local + Decision Log). Keyboard map matches SPEC-004 exactly (`Ctrl/Cmd+K`, `j/k/e/s/a/i/Enter/Esc`).

## Milestones
1. **Tauri v2 shell.** Verify tauri-cli version (A-07), scaffold, window, dark/light theme, `tauri dev` runs. Shell modules: `config.rs` (gateway URL read/write), `keychain.rs` (get/set/delete session token via OS keychain), `cache.rs` (encrypted-at-rest local hot cache - key from keychain, reconstructable so loss is non-fatal). Done when: `pnpm --filter @aether/client tauri dev` launches; keychain round-trips in a shell unit test.
2. **WS client + session.** Connect with token, login flow (username/password/TOTP fields -> gateway), reconnect with backoff (SPEC-006 jitter policy, client-side variant), frame dispatch scaffold. Done when: vitest covers frame dispatch table + reconnect; a login e2e against a gateway stub succeeds.
3. **Shell layout + states.** App frame (nav rail, surface host, status bar with connection + staleness-degradation banner slot per SPEC-004), loading skeletons / empty / error primitives, error-envelope renderer (message + trace_id + retry). Done when: state primitives unit-tested; degradation banner renders on an injected `degradation` frame.
4. **Simple/Advanced toggle.** Session-scoped mode; switching MUST NOT alter data/subscriptions (nothing to alter yet - assert the mechanism doesn't tear down the WS or resubscribe). Done when: e2e proves toggle preserves the connection and pending state (INV-8 mechanism test).
5. **Palette + keyboard router.** Ctrl/Cmd+K fuzzy palette listing registered surfaces/commands; global keyboard router with the SPEC-004 keymap; visible focus, focus order = visual order, Esc-backs-out, no traps. Done when: Playwright keyboard-only navigation across shell surfaces passes; axe/keyboard-trap check clean.
6. **Accessibility baseline.** 200% text scale without loss, AA contrast both themes, reduced-motion honored, no color-only signals in shell chrome. Done when: Playwright 200%-scale snapshot + contrast check pass (SPEC-004 accessibility items).

## Concrete Steps
Run frontend-design SKILL first. Dependencies (Decision-Log each): tauri v2, keyring or tauri keychain plugin, an AEAD crate for cache (age or chacha20poly1305), react, radix-ui primitives, tailwind, a fuzzy matcher (fuse.js), playwright(dev), vitest(dev). Keep the web/secret boundary a hard rule: no secret crosses the Tauri IPC except through the explicit keychain command surface. Commit per milestone.

## Validation and Acceptance
Per-milestone above; `scripts/test-unit.sh` (vitest) + `scripts/test-e2e.sh` green; `verify.sh` -> `verify: ok`; grep audit: no `localStorage`/`sessionStorage`/`window.storage` for secrets, no secret in web layer; `git diff --name-only` matches. Acceptance: SPEC-004 shell/keyboard/mode/accessibility behaviors that don't require feed content are all demonstrated by tests.

## Idempotence and Recovery
Cache loss is non-fatal by design (reconstructable from server) - a test wipes the cache and asserts clean reconnect. `tauri dev` is re-runnable. If tauri-cli version differs from A-07 assumptions, STOP S4 with the version and the specific API delta rather than guessing v1/v2 API shapes.

## Progress
- [ ] M1 Shell  - [ ] M2 WS+session  - [ ] M3 Layout+states  - [ ] M4 Toggle  - [ ] M5 Palette+keyboard  - [ ] M6 Accessibility

## Surprises & Discoveries
(tauri v2 API realities vs training-era docs; keychain plugin quirks)

## Decision Log
(AEAD crate choice; mode-persistence location; fuzzy-matcher choice)

## Outcomes & Retrospective
(shell capabilities demonstrated; a11y evidence; deviations)
