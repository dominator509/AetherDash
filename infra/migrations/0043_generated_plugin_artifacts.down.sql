ALTER TABLE plugin_manifests
    DROP CONSTRAINT IF EXISTS plugin_manifests_generated_artifact_check,
    DROP COLUMN generated,
    DROP COLUMN wasm_artifact;
