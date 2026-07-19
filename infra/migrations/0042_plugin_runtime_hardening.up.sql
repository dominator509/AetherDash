-- EP-403: plugins install pending and require explicit human step-up approval.
ALTER TABLE plugin_manifests
    DROP CONSTRAINT IF EXISTS plugin_manifests_status_check;

-- Legacy rows never acquired the approval evidence required by EP-403.
UPDATE plugin_manifests SET status = 'installed' WHERE status = 'approved';
UPDATE plugin_manifests SET capabilities = '[]'::jsonb WHERE capabilities = '{}'::jsonb;

ALTER TABLE plugin_manifests
    ALTER COLUMN status SET DEFAULT 'installed',
    ALTER COLUMN signature DROP NOT NULL,
    ALTER COLUMN signer DROP NOT NULL,
    ADD COLUMN manifest JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN wasm_hash TEXT NOT NULL DEFAULT repeat('0', 64),
    ADD COLUMN dependency_lock_hash TEXT NOT NULL DEFAULT repeat('0', 64),
    ADD COLUMN approved_by TEXT,
    ADD COLUMN approval_step_up_id TEXT,
    ADD COLUMN granted_capabilities JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN approved_ts TIMESTAMPTZ,
    ADD COLUMN revoked_ts TIMESTAMPTZ,
    ADD CONSTRAINT plugin_manifests_status_check
        CHECK (status IN ('installed','approved','loaded','revoked')),
    ADD CONSTRAINT plugin_manifests_approval_evidence_check
        CHECK (
            status = 'installed'
            OR (approved_by IS NOT NULL AND approval_step_up_id IS NOT NULL AND approved_ts IS NOT NULL
                AND signature IS NOT NULL AND signer IS NOT NULL)
        ),
    ADD CONSTRAINT plugin_manifests_revocation_check
        CHECK (status <> 'revoked' OR revoked_ts IS NOT NULL);

CREATE INDEX idx_plugin_manifests_loadable
    ON plugin_manifests(name, version) WHERE status IN ('approved','loaded');
