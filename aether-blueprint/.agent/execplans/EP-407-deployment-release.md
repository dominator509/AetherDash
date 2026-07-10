Layer: 5 - Execution

# EP-407: Deployment & Release Engineering

**Band:** 4xx Cross-cutting | **Phase:** 4 | **Status:** draft | **Blocked by:** EP-404

## Purpose / Big Picture
Turn the deployment contract into working tooling: automated build + release to the brain host (release dirs, symlink flip, ordered restarts, post-deploy gate), systemd units per service including the hardened Guardian unit, backup/snapshot timers, signing, and the documented client bundle + auto-update path. DEPLOYMENT.md/RELEASE.md/OPERATIONS.md/ROLLBACK.md made executable.

## Scope
Deploy automation (build -> rsync -> migrate -> symlink flip -> ordered restart -> post-deploy gate), systemd unit files per service, backup/snapshot timers (OPERATIONS.md matrix), config-directory snapshots (ROLLBACK.md), artifact signing (minisign + client bundle signing + tauri-updater keys), CI release job, first-deploy bootstrap tooling.

## Non-goals
No prod deploy execution by an agent (STOP S6 - humans deploy; this builds the tooling they run), no cloud-specific IaC (single brain host v1), no zero-downtime rolling (Phase 5 - brief restarts accepted), no live-trading enablement (ceremony, OPERATIONS.md).

## Context and Orientation
DEPLOYMENT.md fixes the topology (desktop client, 24/7 brain host, optional GPU host), the `/opt/aether` release layout, the deploy procedure, and systemd unit conventions; RELEASE.md fixes the gate + signing; OPERATIONS.md fixes backups + timers + the live-trade ceremony; ROLLBACK.md fixes the retained-releases + snapshot needs. This plan implements all of that as tooling. ADR-0009: workflows stay thin (call scripts) so Forgejo migration is a runner swap.

## Files to Read First
1. DEPLOYMENT.md (topology, layout, procedure, units); RELEASE.md (gate, signing, versioning); OPERATIONS.md (backups, timers, ceremony); ROLLBACK.md (retained releases, snapshots).
2. SECURITY.md (Guardian unit hardening T4); EP-306 hardened-unit requirement.

## Files to Change (Expected Changed Files)
`infra/deploy/**` (systemd units `aether-*.service` incl. hardened `aether-wallet-guardian.service`, `compose.prod.yml`, deploy scripts: build/rsync/migrate/flip/restart/post-gate, backup + snapshot timer units, bootstrap script), signing tooling + key management docs, `.github/workflows/release.yml` (thin), COMMANDS.md deploy-related entries, tests-of-tooling (dry-run/staging), CHANGELOG, this file.

## Interfaces and Contracts
Deploy tooling implements DEPLOYMENT.md's procedure exactly (immutable release dirs, atomic symlink flip, ordered restart guardian->risk->router->rest->gateway-last, post-deploy smoke + audit-verify + golden-signal watch); systemd units read `EnvironmentFile`/`LoadCredential` (secrets outside repo); Guardian unit adds `ProtectSystem=strict` + private tmp + sole-writable keystore dir (SECURITY.md); backups per the OPERATIONS.md matrix with retention; signing per RELEASE.md (minisign archives, client bundles, updater keys).

## Milestones
1. **Systemd units.** All `aether-*.service` with dependency ordering (Wants/After), EnvironmentFile/LoadCredential, restart policies; the hardened Guardian unit (ProtectSystem=strict, private tmp, keystore-only-writable). Done when: units validate (`systemd-analyze verify`); a staging bring-up starts services in order; Guardian hardening asserted.
2. **Deploy automation.** build (release binaries + pnpm build + migration snapshot) -> rsync to release dir -> `uv sync` -> preflight -> migrate -> symlink flip -> ordered restart -> post-deploy gate (smoke + audit-verify + golden-signal watch). Done when: a staging deploy runs end-to-end; rollback-compatible layout verified (3 releases retained).
3. **Backups + snapshots.** OPERATIONS.md backup matrix as systemd timers (pg_dump, ClickHouse BACKUP, Kuzu tar, Qdrant snapshot, MinIO mirror) with retention; config-dir snapshot before deploy/ceremony (ROLLBACK.md). Done when: timers run in staging; a restore drill (pg dump -> scratch DB -> sanity) passes; config snapshot/restore works.
4. **Signing + release job.** minisign for archives, client bundle signing, tauri-updater keys; thin `release.yml` (checkout + toolchain + script calls, ADR-0009); version tagging. Done when: signed artifacts verify; release job produces a signed release on a staging tag; client bundle checksum recorded.
5. **First-deploy bootstrap + client + docs.** Bootstrap script (provision skeleton, env files from ENVIRONMENT.md, compose up, migrate, smoke); client bundle build + archived + auto-update channel (signed) documented; Forgejo-runner migration path documented. Done when: bootstrap runs on a fresh staging host to a working paper round-trip; client bundle builds + updater path documented.

## Concrete Steps
Everything here is tooling humans run - agents build and test it on staging, never deploy prod (S6). Workflows stay thin (script calls only) so the Forgejo swap is trivial (ADR-0009). The Guardian unit hardening is non-negotiable (SECURITY.md T4). `live_enabled` is NOT touched by deploy tooling (ceremony, OPERATIONS.md). Backups must include a tested restore (a backup never restored isn't a backup - OPERATIONS.md). Run security-review.md for the unit hardening + secret handling. Commit per milestone.

## Validation and Acceptance
Per-milestone; tooling tests + staging dry-runs green; `verify.sh` + `security-check.sh` green; unit-validate + staging-deploy + restore-drill + signed-artifact tests REQUIRED; `git diff --name-only` matches. Acceptance: PRODUCTION_READINESS deployment items evidencable - deploy procedure executed end-to-end on staging (incl. migrate + flip + gate), rollback drill passes, backup timers + one restore drill done, Guardian hardening applied.

## Idempotence and Recovery
Deploys are atomic (symlink flip) + reversible (retained releases + ROLLBACK.md); bootstrap is re-runnable; backups + snapshots are the recovery substrate (with tested restore). The tooling makes rollback boring by design. Prod execution stays human (S6).

## Progress
- [ ] M1 Systemd units  - [ ] M2 Deploy automation  - [ ] M3 Backups+snapshots  - [ ] M4 Signing+release job  - [ ] M5 Bootstrap+client+docs

## Surprises & Discoveries
(systemd ordering realities; migration-during-deploy edge cases; updater signing)

## Decision Log
(signing key management; backup tooling specifics; updater channel design)

## Outcomes & Retrospective
(staging deploy evidence; restore-drill result; hardening applied)
