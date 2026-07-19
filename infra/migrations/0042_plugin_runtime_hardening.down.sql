DROP INDEX IF EXISTS idx_plugin_manifests_loadable;

ALTER TABLE plugin_manifests
    DROP CONSTRAINT IF EXISTS plugin_manifests_revocation_check,
    DROP CONSTRAINT IF EXISTS plugin_manifests_approval_evidence_check,
    DROP CONSTRAINT IF EXISTS plugin_manifests_status_check;

UPDATE plugin_manifests
SET status = CASE WHEN status IN ('approved','loaded') THEN 'approved' ELSE 'revoked' END;
UPDATE plugin_manifests SET signature = '' WHERE signature IS NULL;
UPDATE plugin_manifests SET signer = '' WHERE signer IS NULL;

ALTER TABLE plugin_manifests
    DROP COLUMN revoked_ts,
    DROP COLUMN approved_ts,
    DROP COLUMN approval_step_up_id,
    DROP COLUMN granted_capabilities,
    DROP COLUMN approved_by,
    DROP COLUMN dependency_lock_hash,
    DROP COLUMN wasm_hash,
    DROP COLUMN manifest,
    ALTER COLUMN signature SET NOT NULL,
    ALTER COLUMN signer SET NOT NULL,
    ALTER COLUMN status SET DEFAULT 'approved',
    ADD CONSTRAINT plugin_manifests_status_check CHECK (status IN ('approved','revoked'));
