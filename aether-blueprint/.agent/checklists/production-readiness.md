Layer: 6 - Verification & Operations

# Checklist: Production Readiness Gate (SPEC-008; EP-408 drives)

- [ ] `scripts/production-readiness-check.sh` runs the full battery (verify + integration + e2e + security + audit + smoke) before the checklist parse.
- [ ] Every `- [ ] REQUIRED` item in PRODUCTION_READINESS.md is checked with real inline evidence (no `Evidence: ...` placeholders on checked items).
- [ ] Time-sensitive evidence (backup streak, cache-hit ratio, audit-verify streak) refreshed within 30 days.
- [ ] RECOMMENDED items either done or deferred with an inline reason + Decision Log revisit trigger.
- [ ] All eleven invariants (INV-1..11) verified under test.
- [ ] Boxes flipped only by a human or the final-review prompt under explicit instruction (bounded autonomy).
- [ ] Any item amendment recorded in Decision Log (removal of a REQUIRED item also has an ADR).
- [ ] Gate output archived in the ops log with the release tag.
