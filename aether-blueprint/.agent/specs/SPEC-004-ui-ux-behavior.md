Layer: 4 - Specification

# SPEC-004: UI/UX Behavior

**Status:** accepted | **Owning plans:** EP-101/102/103/104 | **Last updated:** 2026-07-09

## User-visible goal
A keyboard-first desktop terminal where Simple mode reads like a curated briefing and Advanced mode works like a trading desk - same engine, same data, same commands (INV-8).

## Non-goals
Visual design tokens (EP-101 decides within these behaviors); mobile; web deployment; theming beyond dark/light.

## Terms
**Surface** = a top-level view. **Panel** = a dockable unit inside a surface (Advanced only). **Ticket** = the order-intent form. **Palette** = the fuzzy command launcher.

## Surfaces (both modes unless noted)
1. **Feed** - the opportunity stream. Simple: one-column cards, plain-language headline, net edge, confidence, staleness chip, three actions (Explain / Simulate / Act). Advanced: virtualized table (sortable by net edge, confidence, expiry, venue), multi-select, panel-dockable.
2. **Explain** - layered drill-down: summary sentence -> EdgeDecomposition table -> evidence list (Brain objects with provenance + trust chips) -> raw object viewer. Every layer reachable by keyboard (right/left to descend/ascend).
3. **Simulate** - runs SPEC-012 simulator on an opportunity or manual ticket; shows fill model walk, all decomposition components, and "what changes the answer" sensitivities (size, staleness). Never shows a bare "profit" number without the decomposition one keypress away.
4. **Ticket + Confirm** - order entry. Confirm step displays: market, side, size, limit, quote snapshot age, caps headroom, paper/live badge, tier being exercised. `confirm_required` frames (SPEC-003) render here; TOTP field appears when step-up demanded (SPEC-005).
5. **Command room** (Advanced; Simple gets the palette only) - chat-style NL + slash commands over MCP, with the tier badge always visible and every proposed action rendered as a card requiring the same confirm flow as the ticket. Model output NEVER auto-executes; cards do (INV-1 at the UI layer).
6. **Alerts inbox** - alert history with the same inline actions delivered to external channels.
7. **Positions & P&L** - paper and live strictly visually separated (badge + section), attribution links back to opportunities.
8. **Settings** - caps editor (versioned flow per SPEC-005), venue toggles, channels, tier grants, appearance.

## Keyboard-first (REQUIRED; PRODUCTION_READINESS accessibility items test these)
- Palette on `Ctrl/Cmd+K` from anywhere; every command and surface is reachable through it.
- Feed triage without mouse: `j/k` move, `e` explain, `s` simulate, `a` act, `i` ignore, `Enter` open. Ticket completes and confirms entirely by keyboard, including TOTP.
- Focus is always visible; focus order follows visual order; `Esc` backs out one layer everywhere; no keyboard traps.
- Full path test: feed -> explain -> simulate -> ticket -> confirm executes with zero pointer events (e2e-asserted).

## Mode rules (INV-8)
Mode is a presentation flag on the session. Switching modes MUST NOT alter subscriptions, permissions, data, or pending confirms. Simple hides complexity but never information needed for consent: the confirm step is identical in both modes.

## Staleness & degradation display
Every market-derived number carries age; when age exceeds the freshness threshold (per venue.toml tick expectations), the value renders with a staleness chip and muted styling - color is never the only signal. `degradation` frames (SPEC-003) render as a persistent banner naming the surface and reason; the trading path failing closed shows the router's reason verbatim from the closed reason set.

## Loading / empty / error states (every surface)
Loading = skeletons, never spinners-only; Empty = one plain sentence + the action that fills it; Error = envelope message + trace_id + retry affordance where `retryable`. No raw exceptions ever reach the DOM.

## Accessibility (REQUIRED)
200% text scaling without loss or horizontal scroll; WCAG AA contrast in both themes; no color-only signaling anywhere (chips carry text/icons); every opportunity has a plain-language summary (Simple headline is that summary); reduced-motion honored.

## Performance expectations
Feed virtualization mandatory; 500 live rows update without dropped frames on the reference machine; quote flashes batch at animation-frame cadence; cold start < 3 s to interactive feed (RECOMMENDED gate).

## Security rules
Client renders tiers, never enforces them (SPEC-005); no secrets in client logs or state dumps; the live badge is derived from server state, never client config.

## Required tests
Playwright: full keyboard path; mode-switch invariance (same feed data object identity); confirm-flow with TOTP stub; staleness chip render at threshold; degradation banner on injected `degradation` frame; 200% scaling snapshot. Vitest: EdgeDecomposition table renders all 11 components incl. explicit zeros.

## Acceptance criteria
All required tests green in `scripts/test-e2e.sh` / `test-unit.sh`; PRODUCTION_READINESS accessibility items evidencable from these tests.
