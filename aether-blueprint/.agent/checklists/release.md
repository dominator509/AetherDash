Layer: 6 - Verification & Operations

# Checklist: Release (RELEASE.md gate; human-run)

- [ ] `scripts/verify.sh`, `test-integration.sh`, `test-e2e.sh` all pass on the release commit.
- [ ] `scripts/security-check.sh` and `dependency-audit.sh` clean.
- [ ] Audit chain full verify passes (post-Phase-2).
- [ ] `.agent/PLANS.md` shows no plan `active` (quiescent tree).
- [ ] CHANGELOG Unreleased rolled into the new version heading.
- [ ] Version bumped correctly (MINOR = features, PATCH = fixes); tag `aether-v0.M.P` prepared.
- [ ] `AETHER_EXECUTION__LIVE_ENABLED` state noted in the release ops-log entry (post-ceremony releases).
- [ ] Artifacts built; client bundle checksums recorded; signing applied where required.
- [ ] Deploy per DEPLOYMENT.md; post-deploy smoke + golden-signal watch done.
- [ ] Ops-log entry complete (version, deployer, gate evidence, post-deploy note).
