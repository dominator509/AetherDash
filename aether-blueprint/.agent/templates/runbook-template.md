Layer: 6 - Verification & Operations

# RB-XXX: <Service / Symptom>

**Service:** <name> | **Severity when firing:** SEV1|2|3 | **Owning plan:** EP-XXX

## Symptom / alert
Exact alert name or observable symptom.

## Impact
What the operator loses while this is broken (trading path? data freshness? alerts?).

## Diagnosis
Ordered checks with exact commands/queries (health endpoints, `docker compose logs <svc>`, ClickHouse/Postgres queries, bus lag).

## Remediation
Ordered, safest-first actions. Mark any step that is irreversible or touches money/keys - those require the operator, never an agent.

## Rollback pointer
Link to ROLLBACK.md section if remediation includes a version rollback.

## Escalation
When to stop self-healing and page the operator; what evidence to attach.

## Post-incident verification
Commands proving recovery, plus the audit/metric entries to confirm.
