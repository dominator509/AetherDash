Layer: 6 - Verification & Operations

# RELEASE.md - Versioning and Release Gate

## Versioning
- Single repo-wide version `0.MINOR.PATCH` during v1 (SemVer semantics; 1.0.0 = production-readiness gate green). Git tag `aether-v0.M.P` on the release commit.
- All services in a release share the version (monorepo, ADR-0001); the client bundle carries the same version string.
- MINOR = new ExecPlan(s) completed / behavior added; PATCH = fixes only. Breaking proto changes are additive-only inside v1 (SPEC-003); a true breaking change forces a `v2` proto package, not a version-number trick.

## Changelog
`CHANGELOG.md` in Keep-a-Changelog format, newest first: Added / Changed / Fixed / Security sections. Each entry names its ExecPlan (EP-XXX) and, for behavior changes, its spec. The Unreleased section is updated by the plan that makes the change (EXECUTION_RULES R13), not reconstructed at release time.

## Release gate (all must pass before tagging; humans tag)
1. `scripts/verify.sh` -> `verify: ok`; `scripts/test-integration.sh`; `scripts/test-e2e.sh`.
2. `scripts/security-check.sh` and `scripts/dependency-audit.sh` clean.
3. Audit chain full verify (not just incremental) passes: `aether-audit verify --full` (tool lands in EP-402; until then this line blocks releases past Phase 2 only).
4. `.agent/PLANS.md` shows no plan in `active` state (release from a quiescent tree).
5. CHANGELOG Unreleased section reviewed and rolled into the new version heading.
6. For releases after the live-trading ceremony: `AETHER_EXECUTION__LIVE_ENABLED` state explicitly noted in the release ops-log entry.

## Release procedure
1. Gate above on the release commit.
2. `git tag aether-v0.M.P && git push --tags` (remote policy per A-17).
3. Build artifacts and deploy per DEPLOYMENT.md; archive client bundles alongside the server release dir.
4. Ops-log entry: version, deployer, gate evidence links, post-deploy golden-signal note.

## Hotfix path
Branch from the release tag, fix + tests, PATCH bump, run the full gate (no shortcuts - the gate is fast by design), deploy. Cherry-pick back to main the same day.

## Artifact integrity
Release dirs are immutable after deploy (DEPLOYMENT.md). Client bundles get checksums recorded in the ops log; signing (minisign for archives, OS-native for client bundles + tauri-updater keys) is wired in EP-407 and REQUIRED before any auto-update channel exists.
