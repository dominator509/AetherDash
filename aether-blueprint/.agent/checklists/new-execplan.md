Layer: 6 - Verification & Operations

# Checklist: Authoring a New ExecPlan

Use when creating a plan from `.agent/templates/execplan-template.md`.

- [ ] Placed in the correct band (0xx/1xx/2xx/3xx/4xx) and registered in `.agent/PLANS.md` with band, phase, status, blocked-by.
- [ ] Purpose states what exists after the plan that didn't before.
- [ ] Scope is concrete; Non-goals list the tempting adjacent work explicitly.
- [ ] Owning spec(s) identified and read; behavior comes from the spec, not the plan.
- [ ] Files to Read First ordered with reasons.
- [ ] Expected Changed Files exhaustive and within the plane band (D1-D7 respected).
- [ ] Interfaces name exact proto messages / bus topics / routes / tables, each registered where ARCHITECTURE.md says.
- [ ] Milestones are 6-10, each independently validatable, in dependency order.
- [ ] Concrete Steps precise enough for a lower-tier agent (no guessing); commands referenced by COMMANDS.md name.
- [ ] Validation gives exact command + expected output per milestone; plan-level acceptance is machine-checkable.
- [ ] Idempotence/Recovery covers re-runs, unexpected repo states, and which failures are STOP vs retry.
- [ ] Relevant invariants (INV-x) named in Context.
- [ ] Plan stays small; if >10 milestones, split before starting.
