DROP TRIGGER IF EXISTS guardian_broadcast_tx_hash_bound ON guardian_proposals;
DROP FUNCTION IF EXISTS protect_guardian_broadcast_tx_hash();
DROP TRIGGER IF EXISTS guardian_broadcast_job_immutable ON guardian_broadcast_jobs;
DROP FUNCTION IF EXISTS protect_guardian_broadcast_job();
DROP TABLE IF EXISTS guardian_broadcast_jobs;
DROP TABLE IF EXISTS guardian_chain_nonces;
