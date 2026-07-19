-- EP-406: generated code is compiled then stored inert until EP-403 approval.
ALTER TABLE plugin_manifests
    ADD COLUMN wasm_artifact BYTEA,
    ADD COLUMN generated BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE plugin_manifests
    ADD CONSTRAINT plugin_manifests_generated_artifact_check
        CHECK (NOT generated OR wasm_artifact IS NOT NULL);
