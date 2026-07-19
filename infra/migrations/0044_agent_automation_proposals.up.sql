-- EP-406: restart-safe automation budgets and inert human-review proposals.
CREATE TABLE agent_automations (
    id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL,
    grant_id TEXT NOT NULL,
    action JSONB NOT NULL,
    cron JSONB NOT NULL,
    max_runs BIGINT NOT NULL CHECK (max_runs > 0),
    max_cost_minor BIGINT NOT NULL CHECK (max_cost_minor >= 0),
    cost_per_run_minor BIGINT NOT NULL CHECK (
        cost_per_run_minor >= 0 AND cost_per_run_minor <= max_cost_minor
    ),
    runs BIGINT NOT NULL DEFAULT 0 CHECK (runs >= 0 AND runs <= max_runs),
    cost_minor BIGINT NOT NULL DEFAULT 0 CHECK (cost_minor >= 0 AND cost_minor <= max_cost_minor),
    last_slot BIGINT,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','paused','revoked')),
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE improvement_proposals (
    id TEXT PRIMARY KEY,
    summary TEXT NOT NULL,
    unified_diff TEXT NOT NULL,
    evidence JSONB NOT NULL,
    digest TEXT NOT NULL CHECK (length(digest) = 64),
    status TEXT NOT NULL DEFAULT 'proposed'
        CHECK (status IN ('proposed','application_authorized','rejected')),
    authorized_by TEXT,
    authorized_ts TIMESTAMPTZ,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (status <> 'application_authorized' OR authorized_by IS NOT NULL)
);
