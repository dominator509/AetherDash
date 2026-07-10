Layer: 4 - Specification

# SPEC-008: Production Readiness Gate

**Status:** accepted | **Owning plans:** EP-408 (primary) | **Last updated:** 2026-07-09

## User-visible goal
"Production-ready" is a script exit code plus an evidence trail, not a feeling. This spec defines the mechanics; PRODUCTION_READINESS.md holds the items; EP-408 drives them green.

## Non-goals
Defining the checklist items (they live in PRODUCTION_READINESS.md and change only via Decision Log + this spec's amendment rule); CI release automation (RELEASE.md).

## Terms
**Gate** = `scripts/production-readiness-check.sh`. **Item** = a checklist line. **Evidence** = the inline reference after a checked item.

## Required behavior
1. The gate MUST fail while any `- [ ] REQUIRED` line exists, and MUST run the full verification battery (verify, integration, e2e, security, dependency-audit, smoke) before the checklist parse - a green checklist with red tests is meaningless.
2. Checking a REQUIRED item MUST append evidence inline: a command + date, an ops-log line reference, an audit event id, or a metric snapshot ref. Bare `[x]` with `Evidence: ...` still reading `...` is a defect final-review must catch (pattern is grep-able: `Evidence: \.\.\.` on a checked line).
3. Only a human or the final-review prompt acting on explicit human instruction may check items; execute/continue prompts may gather evidence but never flip boxes (bounded autonomy - the gate is a human artifact).
4. RECOMMENDED items may be deferred only with an inline reason and a Decision Log entry naming the revisit trigger.
5. Items are amended (added/removed/reworded) only through a Decision Log entry in the plan doing it; removing a REQUIRED item additionally needs an ADR (it is a scope decision).
6. The gate re-runs per release (RELEASE.md gate step) - readiness decays; evidence older than 30 days for time-sensitive items (backup-timer streak, cache-hit ratio, audit-verify streak) MUST be refreshed, and those items say so inline.
7. Phase exits (ROADMAP.md) MUST NOT claim readiness language; only this gate does. A phase exit checks its own criteria; the gate checks the union at the end.

## Inputs / Outputs
Inputs: battery results, checklist file, ops log, metrics. Output: exit 0 + `production readiness: ok`, or exit 1 naming unchecked/stale items.

## Error states
Checklist file missing/unparseable -> fail loudly (already implemented). Battery failure -> fail before parsing. Evidence-placeholder pattern found on a checked REQUIRED line -> EP-408 adds this grep to the script (amendment via this spec's rule 5, pre-authorized here).

## Security rules
Evidence references never contain secret values; metric snapshots are numbers + timestamps, not config dumps.

## Required tests
Script-behavior tests (shell-level, in EP-408): unchecked-REQUIRED fails; checked-with-placeholder-evidence fails (once rule-5 grep lands); RECOMMENDED-unchecked passes; battery-failure short-circuits. Plus one full dry run on a staging copy of the checklist with synthetic evidence.

## Acceptance criteria
EP-408 done = gate behavior tests green, every REQUIRED item checked with real evidence or the release consciously blocked, and the final gate output archived in the ops log with the release tag.
