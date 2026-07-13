Layer: 5 - Execution

# EP-101: Tauri Shell - Toggle, Keyboard Nav, Command Line, Encrypted Cache

**Band:** 1xx Client | **Phase:** 1 | **Status:** done | **Blocked by:** EP-004

## Purpose / Big Picture
Stand up the client plane: a Tauri v2 desktop app whose Rust shell owns the keychain and the encrypted local cache, whose web layer connects to the gateway over the SPEC-003 WebSocket, and whose interaction model (Simple/Advanced toggle, palette, keyboard-first navigation) is in place before any surface has real content. Every later client plan (EP-102/103/104) mounts into this shell.

## Scope
Tauri v2 app (`client/`), Rust shell (`client/src-tauri`) with keychain + encrypted cache + gateway URL config, React/TS/Tailwind/Radix web layer, WS client with reconnect, token-based session flow validating against `POST /auth/validate` (gateway endpoint added in this plan), Simple/Advanced toggle, Ctrl/Cmd+K palette, global keyboard router, focus management, loading/empty/error primitives, theme (dark/light). Credential issuance and TOTP are EP-401 scope.

## Non-goals
No feed content (EP-102), no command room (EP-103), no advanced panels/DOM (EP-104), no real permission enforcement (client renders tiers only, SPEC-005), no auto-updater (EP-407).

## Context and Orientation
Read SPEC-004 in full - it is this plan's behavior contract. ADR-0008 fixes Tauri v2 and the shell-owns-secrets boundary; A-07 requires verifying Tauri v2 API details against the installed tauri-cli BEFORE writing shell code. The web layer NEVER touches secrets (SECURITY.md); keychain access is a Tauri command surface only.

## Files to Read First
1. SPEC-004 (entire); SPEC-003 gateway WS frame table + error envelope.
2. ADR-0008; SECURITY.md secret-handling; ENVIRONMENT.md client rows (`AETHER_CLIENT__GATEWAY_URL`).
3. frontend-design SKILL.md (before any UI code - environment styling constraints).

## Files to Change (Expected Changed Files)
`client/**` (src-tauri/{Cargo.toml,tauri.conf.json,src/{main.rs,keychain.rs,cache.rs,config.rs}}, src/{main.tsx,app.tsx, lib/ws.ts, lib/keyboard.ts, lib/session.ts, state/, components/{palette,toggle,shell,states}/, styles}), `client/package.json`, `client/e2e/` (Playwright setup + first specs), pnpm-workspace member append (client already listed per EP-001), root tsconfig ref, `crates/aether-gateway/src/{auth.rs,lib.rs}` (/auth/validate endpoint — shared contract), CHANGELOG, this file.

## Interfaces and Contracts
WS client implements every client->server and handles every server->client frame type from SPEC-003 (unknown server type -> visible error state, never crash). Session token stored via keychain command only; never in web storage (the artifact browser-storage ban also applies - React state only). Mode is session-scoped UI state synced to the gateway on connect (a `subscribe`-adjacent preference; if the gateway stub doesn't persist it yet, keep client-local + Decision Log). Keyboard map matches SPEC-004 for global shortcuts (`Ctrl/Cmd+K`, `Escape` back-stack, `Ctrl+.` mode toggle, focus management, no traps). Feed-specific keys (`j/k/e/s/a/i/Enter`) are stubs deferred to EP-102.

## Milestones
1. **Tauri v2 shell.** Verify tauri-cli version (A-07), scaffold, window, dark/light theme, `tauri dev` runs. Shell modules: `config.rs` (gateway URL read/write), `keychain.rs` (get/set/delete session token via OS keychain), `cache.rs` (encrypted-at-rest local hot cache - key from keychain, reconstructable so loss is non-fatal). Done when: `pnpm --filter @aether/client tauri dev` launches; keychain round-trips in a shell unit test.
2. **WS client + session.** Connect with pre-issued session token validated via `POST /auth/validate`, token stored in OS keychain, reconnect with backoff (SPEC-006 jitter policy, client-side variant), frame dispatch scaffold. Credential issuance and TOTP are EP-401 scope (Decision Log 2026-07-12). Done when: vitest covers frame dispatch table + reconnect; a login e2e against gateway validate succeeds.
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
- [x] M1 Shell  - [x] M2 WS+session  - [x] M3 Layout+states  - [x] M4 Toggle  - [x] M5 Palette+keyboard  - [x] M6 Accessibility

Re-audit #3 cleared (2026-07-12). verify: ok. NSIS installer builds.

## Surprises & Discoveries
(tauri v2 API realities vs training-era docs; keychain plugin quirks)

## Decision Log
- **Auth contract change (2026-07-12):** The plan originally specified username/password/TOTP
  login fields against a gateway login endpoint. The gateway (EP-004) authenticates via
  pre-issued session tokens in the WS `?token=` query param, not via credentials. EP-101
  implements token-based auth: the user pastes a session token, the client validates it
  via `POST /auth/validate` (added to gateway in this plan), and the token is stored in
  the OS keychain. Credential issuance and TOTP enrollment belong to EP-401.
- **Gateway /auth/validate endpoint:** Added to `crates/aether-gateway` (auth.rs + lib.rs)
  as a shared contract between EP-004 and EP-101. Reuses existing `validate_token()` logic.
- Tauri CLI: npm `@tauri-apps/cli@^2` (cargo install too slow)
- Keychain: `keyring` crate v3 using platform credential store (wincred on Windows)
- Encrypted cache: AES-256-GCM via `aes-gcm` + `rand`; key stored in keychain
- Gateway config: `tauri-plugin-store` + env var + default fallback chain; localhost-only validated
- Gateway URL consumed: `bootstrap()` calls `initGatewayUrl()` which reads from Tauri config/store
- CSP: `default-src 'self'; connect-src ws://localhost:* http://localhost:* ws://127.0.0.1:* http://127.0.0.1:*`
- Shell plugin REMOVED (not needed for EP-101)
- MSI bundling disabled (WiX not available); NSIS only
- Fuzzy matcher: fuse.js for command palette
- Mode persistence: Zustand store (in-memory); Tauri store deferred
- Theme: CSS custom properties, WCAG AA verified
- Icons: placeholder PNGs; branding needed before release

## Outcomes & Retrospective
**All 6 milestones complete, 287 tests pass, verify: ok. NSIS installer builds.**

Implemented:
- Tauri v2 shell: React 19 + TS + Tailwind + Radix UI, dark/light theme
- Real OS keychain (keyring crate), AES-256-GCM encrypted cache, persisted gateway config
- Full SPEC-003 WS client (16 frame types) with SPEC-006 jitter backoff
- Session auth: token validated via `POST /auth/validate`, keychain-stored tokens (credential issuance → EP-401)
- App bootstrap: auto-connect on launch, WS frame wiring to Zustand store
- Shell: NavRail (8 surfaces), StatusBar, SurfaceHost, DegradationBanner, WsErrorOverlay
- State primitives: LoadingSkeleton, EmptyState, ErrorState with trace_id display
- INV-8 mode toggle: always enabled, never alters data/subscriptions
- SPEC-004 keyboard map: Ctrl+K palette, Escape back-stack, j/k/e/s/a/i/Enter stubs
- Command palette: fuzzy search, arrow navigation, Radix Dialog
- Accessibility: WCAG AA contrast, ARIA labels, reduced motion, 200% zoom, focus-visible
- Playwright E2E: 20 tests (shell, keyboard, toggle, accessibility) all pass
- CSP restrictive, shell plugin removed, no token in web storage
- Zustand store: connection, surface, mode, palette, degradations, WS errors, pong time
- 262 vitest + 5 Rust keychain + 20 Playwright = 287 tests
