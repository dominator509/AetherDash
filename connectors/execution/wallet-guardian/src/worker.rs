//! Restart-safe Guardian-custody broadcast and receipt reconciliation.

use crate::broadcast::{raw_transaction_hash, sign_eip1559_transaction, BroadcastError};
use crate::keystore::KeyStore;
use crate::proposal::TxSpec;
use crate::rpc::RpcClient;
use aether_core::ids::Ulid;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

const WORKER_ACTOR: &str = "guardian-worker";

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("Guardian broadcast persistence is unavailable")]
    Database(#[from] sqlx::Error),
    #[error("Guardian transaction preparation failed")]
    Broadcast(#[from] BroadcastError),
    #[error("Guardian proposal payload is invalid")]
    InvalidProposal,
    #[error("Guardian chain identifier is unsupported")]
    UnsupportedChain,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WorkerReport {
    pub expired: usize,
    pub prepared: usize,
    pub submitted: usize,
    pub confirmed: usize,
    pub failed: usize,
    pub abandoned: usize,
    pub deferred: usize,
}

#[derive(Clone)]
pub struct BroadcastWorker {
    pool: PgPool,
    keystore: Arc<KeyStore>,
    poll_interval: Duration,
    batch_size: i64,
}

#[derive(Debug)]
struct Job {
    proposal_id: String,
    chain_id: i64,
    nonce: i64,
    signed_raw: String,
    tx_hash: String,
    job_state: String,
    proposal_state: String,
    approval_expires_at: Option<DateTime<Utc>>,
    grant_id: String,
}

impl BroadcastWorker {
    pub fn from_env(pool: PgPool, keystore: Arc<KeyStore>) -> Result<Self, WorkerError> {
        let poll_ms = std::env::var("AETHER_GUARDIAN__WORKER_POLL_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| (100..=60_000).contains(value))
            .unwrap_or(1_000);
        let batch_size = std::env::var("AETHER_GUARDIAN__WORKER_BATCH_SIZE")
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| (1..=100).contains(value))
            .unwrap_or(10);
        Ok(Self { pool, keystore, poll_interval: Duration::from_millis(poll_ms), batch_size })
    }

    #[cfg(test)]
    pub fn for_test(pool: PgPool, keystore: Arc<KeyStore>) -> Self {
        Self { pool, keystore, poll_interval: Duration::from_millis(10), batch_size: 10 }
    }

    /// Run forever. Individual RPC failures are persisted as bounded retries;
    /// database failures defer the cycle without terminating the signer.
    pub async fn run(self) {
        let mut ticker = tokio::time::interval(self.poll_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            if self.run_once().await.is_err() {
                tracing::warn!("Guardian broadcast cycle deferred");
            }
        }
    }

    pub async fn run_once(&self) -> Result<WorkerReport, WorkerError> {
        let mut report = WorkerReport::default();
        self.expire_unclaimed(&mut report).await?;
        self.reconcile_due(&mut report).await?;
        self.prepare_approved(&mut report).await?;
        self.reconcile_due(&mut report).await?;
        Ok(report)
    }

    async fn expire_unclaimed(&self, report: &mut WorkerReport) -> Result<(), WorkerError> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT p.id FROM guardian_proposals p \
             WHERE ((p.state='pending' AND p.expires_ts<=now()) \
                    OR (p.state IN ('approved','auto_approved') \
                        AND (p.approval_expires_at IS NULL OR p.approval_expires_at<=now()))) \
               AND NOT EXISTS (SELECT 1 FROM guardian_broadcast_jobs j WHERE j.proposal_id=p.id) \
             ORDER BY p.updated_ts,p.id LIMIT $1",
        )
        .bind(self.batch_size)
        .fetch_all(&self.pool)
        .await?;
        for id in ids {
            let mut db = self.pool.begin().await?;
            let row = sqlx::query(
                "SELECT state,grant_id,expires_ts,approval_expires_at \
                 FROM guardian_proposals WHERE id=$1 FOR UPDATE",
            )
            .bind(&id)
            .fetch_optional(&mut *db)
            .await?;
            let Some(row) = row else {
                db.rollback().await?;
                continue;
            };
            let state: String = row.get("state");
            let grant_id: String = row.get("grant_id");
            let stale = if state == "pending" {
                row.get::<DateTime<Utc>, _>("expires_ts") <= Utc::now()
            } else if matches!(state.as_str(), "approved" | "auto_approved") {
                row.get::<Option<DateTime<Utc>>, _>("approval_expires_at")
                    .is_none_or(|expiry| expiry <= Utc::now())
            } else {
                false
            };
            let has_job: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM guardian_broadcast_jobs WHERE proposal_id=$1)",
            )
            .bind(&id)
            .fetch_one(&mut *db)
            .await?;
            if stale && !has_job {
                expire_proposal(&mut db, &id, &state, &grant_id, "proposal_expired").await?;
                db.commit().await?;
                report.expired += 1;
            } else {
                db.rollback().await?;
            }
        }
        Ok(())
    }

    async fn prepare_approved(&self, report: &mut WorkerReport) -> Result<(), WorkerError> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT p.id FROM guardian_proposals p \
             WHERE p.custody_mode='guardian_custody' \
               AND p.state IN ('approved','auto_approved') \
               AND NOT EXISTS (SELECT 1 FROM guardian_broadcast_jobs j WHERE j.proposal_id=p.id) \
             ORDER BY p.approved_at, p.id LIMIT $1",
        )
        .bind(self.batch_size)
        .fetch_all(&self.pool)
        .await?;
        for id in ids {
            if self.prepare_one(&id).await? {
                report.prepared += 1;
            }
        }
        Ok(())
    }

    async fn prepare_one(&self, proposal_id: &str) -> Result<bool, WorkerError> {
        let candidate = sqlx::query(
            "SELECT tx_spec FROM guardian_proposals WHERE id=$1 \
             AND custody_mode='guardian_custody' \
             AND state IN ('approved','auto_approved')",
        )
        .bind(proposal_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(candidate) = candidate else {
            return Ok(false);
        };
        let tx: TxSpec = serde_json::from_value(candidate.get("tx_spec"))
            .map_err(|_| WorkerError::InvalidProposal)?;
        let chain_id = i64::try_from(tx.chain_id).map_err(|_| WorkerError::UnsupportedChain)?;
        let rpc = match rpc_for_chain(tx.chain_id) {
            Some(rpc) => rpc,
            None => return Ok(false),
        };
        let pending_nonce = rpc
            .eth_get_pending_transaction_count(self.keystore.address().as_str())
            .await
            .ok()
            .and_then(|nonce| i64::try_from(nonce).ok());
        let Some(pending_nonce) = pending_nonce else {
            return Ok(false);
        };

        let mut db = self.pool.begin().await?;
        let proposal = sqlx::query(
            "SELECT state, approval_expires_at, grant_id, tx_spec \
             FROM guardian_proposals WHERE id=$1 FOR UPDATE",
        )
        .bind(proposal_id)
        .fetch_optional(&mut *db)
        .await?;
        let Some(proposal) = proposal else {
            db.rollback().await?;
            return Ok(false);
        };
        let state: String = proposal.get("state");
        let approval_expires_at: Option<DateTime<Utc>> = proposal.get("approval_expires_at");
        let grant_id: String = proposal.get("grant_id");
        if !matches!(state.as_str(), "approved" | "auto_approved") {
            db.rollback().await?;
            return Ok(false);
        }
        if approval_expires_at.is_none_or(|expiry| expiry <= Utc::now()) {
            expire_proposal(&mut db, proposal_id, &state, &grant_id, "approval_expired").await?;
            db.commit().await?;
            return Ok(false);
        }
        let already_prepared: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM guardian_broadcast_jobs WHERE proposal_id=$1)",
        )
        .bind(proposal_id)
        .fetch_one(&mut *db)
        .await?;
        if already_prepared {
            db.rollback().await?;
            return Ok(false);
        }

        // Serialize nonce allocation per chain across worker processes.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(tx.chain_id.to_string())
            .execute(&mut *db)
            .await?;
        let chain_has_prepared_job: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM guardian_broadcast_jobs \
             WHERE chain_id=$1 AND state='prepared')",
        )
        .bind(chain_id)
        .fetch_one(&mut *db)
        .await?;
        if chain_has_prepared_job {
            db.rollback().await?;
            return Ok(false);
        }
        let durable_next: Option<i64> = sqlx::query_scalar(
            "SELECT next_nonce FROM guardian_chain_nonces WHERE chain_id=$1 FOR UPDATE",
        )
        .bind(chain_id)
        .fetch_optional(&mut *db)
        .await?;
        let nonce = durable_next.unwrap_or(pending_nonce).max(pending_nonce);
        let nonce_u64 = u64::try_from(nonce).map_err(|_| WorkerError::UnsupportedChain)?;
        let signed_raw = sign_eip1559_transaction(&self.keystore, &tx, nonce_u64, tx.chain_id)?;
        let tx_hash = raw_transaction_hash(&signed_raw)?;

        sqlx::query(
            "INSERT INTO guardian_broadcast_jobs \
             (proposal_id,chain_id,nonce,signed_raw,tx_hash,state) \
             VALUES ($1,$2,$3,$4,$5,'prepared')",
        )
        .bind(proposal_id)
        .bind(chain_id)
        .bind(nonce)
        .bind(&signed_raw)
        .bind(&tx_hash)
        .execute(&mut *db)
        .await?;
        sqlx::query(
            "INSERT INTO guardian_chain_nonces (chain_id,next_nonce) VALUES ($1,$2) \
             ON CONFLICT (chain_id) DO UPDATE \
             SET next_nonce=GREATEST(guardian_chain_nonces.next_nonce,EXCLUDED.next_nonce), \
                 updated_ts=now()",
        )
        .bind(chain_id)
        .bind(nonce.checked_add(1).ok_or(WorkerError::UnsupportedChain)?)
        .execute(&mut *db)
        .await?;
        insert_worker_event(
            &mut db,
            proposal_id,
            Some(&state),
            &state,
            &grant_id,
            "broadcast_prepared",
        )
        .await?;
        db.commit().await?;
        Ok(true)
    }

    async fn reconcile_due(&self, report: &mut WorkerReport) -> Result<(), WorkerError> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT proposal_id FROM guardian_broadcast_jobs \
             WHERE state IN ('prepared','submitted') AND next_attempt_ts<=now() \
             ORDER BY next_attempt_ts, proposal_id LIMIT $1",
        )
        .bind(self.batch_size)
        .fetch_all(&self.pool)
        .await?;
        for id in ids {
            self.reconcile_one(&id, report).await?;
        }
        Ok(())
    }

    async fn reconcile_one(
        &self,
        proposal_id: &str,
        report: &mut WorkerReport,
    ) -> Result<(), WorkerError> {
        let row = sqlx::query(
            "SELECT j.proposal_id,j.chain_id,j.nonce,j.signed_raw,j.tx_hash,j.state AS job_state, \
                    p.state AS proposal_state,p.approval_expires_at,p.grant_id \
             FROM guardian_broadcast_jobs j JOIN guardian_proposals p ON p.id=j.proposal_id \
             WHERE j.proposal_id=$1",
        )
        .bind(proposal_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(());
        };
        let job = Job {
            proposal_id: row.get("proposal_id"),
            chain_id: row.get("chain_id"),
            nonce: row.get("nonce"),
            signed_raw: row.get("signed_raw"),
            tx_hash: row.get("tx_hash"),
            job_state: row.get("job_state"),
            proposal_state: row.get("proposal_state"),
            approval_expires_at: row.get("approval_expires_at"),
            grant_id: row.get("grant_id"),
        };
        let chain_id = u64::try_from(job.chain_id).map_err(|_| WorkerError::UnsupportedChain)?;
        let Some(rpc) = rpc_for_chain(chain_id) else {
            self.defer_job(&job.proposal_id, "rpc_not_configured").await?;
            report.deferred += 1;
            return Ok(());
        };

        match rpc.eth_transaction_receipt(&job.tx_hash).await {
            Ok(Some(success)) => {
                self.finish_job(&job, success).await?;
                if success {
                    report.confirmed += 1;
                } else {
                    report.failed += 1;
                }
                return Ok(());
            }
            Ok(None) => {}
            Err(_) => {
                self.defer_job(&job.proposal_id, "receipt_unavailable").await?;
                report.deferred += 1;
                return Ok(());
            }
        }

        match rpc.eth_transaction_known(&job.tx_hash).await {
            Ok(true) => {
                self.mark_submitted(&job, false).await?;
                report.submitted += 1;
                return Ok(());
            }
            Ok(false) => {}
            Err(_) => {
                self.defer_job(&job.proposal_id, "transaction_lookup_unavailable").await?;
                report.deferred += 1;
                return Ok(());
            }
        }

        if job.job_state == "prepared"
            && (job.proposal_state == "expired"
                || job.approval_expires_at.is_none_or(|expiry| expiry <= Utc::now()))
        {
            self.abandon_job(&job).await?;
            report.abandoned += 1;
            return Ok(());
        }
        if !matches!(job.proposal_state.as_str(), "approved" | "auto_approved" | "broadcast") {
            self.defer_job(&job.proposal_id, "proposal_state_changed").await?;
            report.deferred += 1;
            return Ok(());
        }

        match rpc.eth_send_raw_transaction(&job.signed_raw).await {
            Ok(returned_hash) if returned_hash.eq_ignore_ascii_case(&job.tx_hash) => {
                self.mark_submitted(&job, true).await?;
                report.submitted += 1;
            }
            Ok(_) => {
                self.defer_job(&job.proposal_id, "rpc_hash_mismatch").await?;
                report.deferred += 1;
            }
            Err(_) => match rpc.eth_transaction_known(&job.tx_hash).await {
                Ok(true) => {
                    self.mark_submitted(&job, true).await?;
                    report.submitted += 1;
                }
                _ => {
                    self.defer_job(&job.proposal_id, "broadcast_unavailable").await?;
                    report.deferred += 1;
                }
            },
        }
        Ok(())
    }

    async fn mark_submitted(&self, job: &Job, attempted: bool) -> Result<(), WorkerError> {
        let mut db = self.pool.begin().await?;
        let current: String = sqlx::query_scalar(
            "SELECT state FROM guardian_broadcast_jobs WHERE proposal_id=$1 FOR UPDATE",
        )
        .bind(&job.proposal_id)
        .fetch_one(&mut *db)
        .await?;
        if !matches!(current.as_str(), "prepared" | "submitted") {
            db.rollback().await?;
            return Ok(());
        }
        sqlx::query(
            "UPDATE guardian_broadcast_jobs SET state='submitted', \
             attempts=attempts+$2, last_attempt_ts=CASE WHEN $2=1 THEN now() ELSE last_attempt_ts END, \
             next_attempt_ts=now()+INTERVAL '2 seconds',last_error_code=NULL,updated_ts=now() \
             WHERE proposal_id=$1",
        )
        .bind(&job.proposal_id)
        .bind(if attempted { 1_i32 } else { 0_i32 })
        .execute(&mut *db)
        .await?;
        transition_to_broadcast(&mut db, job).await?;
        db.commit().await?;
        Ok(())
    }

    async fn finish_job(&self, job: &Job, success: bool) -> Result<(), WorkerError> {
        let mut db = self.pool.begin().await?;
        let current: String = sqlx::query_scalar(
            "SELECT state FROM guardian_broadcast_jobs WHERE proposal_id=$1 FOR UPDATE",
        )
        .bind(&job.proposal_id)
        .fetch_one(&mut *db)
        .await?;
        if !matches!(current.as_str(), "prepared" | "submitted") {
            db.rollback().await?;
            return Ok(());
        }
        transition_to_broadcast(&mut db, job).await?;
        let terminal = if success { "confirmed" } else { "failed" };
        let proposal_state: String =
            sqlx::query_scalar("SELECT state FROM guardian_proposals WHERE id=$1 FOR UPDATE")
                .bind(&job.proposal_id)
                .fetch_one(&mut *db)
                .await?;
        if proposal_state == "broadcast" {
            sqlx::query("UPDATE guardian_proposals SET state=$2,updated_ts=now() WHERE id=$1")
                .bind(&job.proposal_id)
                .bind(terminal)
                .execute(&mut *db)
                .await?;
            insert_worker_event(
                &mut db,
                &job.proposal_id,
                Some("broadcast"),
                terminal,
                &job.grant_id,
                if success { "receipt_succeeded" } else { "receipt_reverted" },
            )
            .await?;
        }
        sqlx::query(
            "UPDATE guardian_broadcast_jobs SET state=$2,next_attempt_ts=now(), \
             last_error_code=NULL,updated_ts=now() WHERE proposal_id=$1",
        )
        .bind(&job.proposal_id)
        .bind(terminal)
        .execute(&mut *db)
        .await?;
        db.commit().await?;
        Ok(())
    }

    async fn abandon_job(&self, job: &Job) -> Result<(), WorkerError> {
        let mut db = self.pool.begin().await?;
        let current: String = sqlx::query_scalar(
            "SELECT state FROM guardian_broadcast_jobs WHERE proposal_id=$1 FOR UPDATE",
        )
        .bind(&job.proposal_id)
        .fetch_one(&mut *db)
        .await?;
        if current != "prepared" {
            db.rollback().await?;
            return Ok(());
        }
        sqlx::query(
            "UPDATE guardian_broadcast_jobs SET state='abandoned', \
             last_error_code='approval_expired',updated_ts=now() WHERE proposal_id=$1",
        )
        .bind(&job.proposal_id)
        .execute(&mut *db)
        .await?;
        sqlx::query(
            "UPDATE guardian_chain_nonces SET next_nonce=$2,updated_ts=now() \
             WHERE chain_id=$1 AND next_nonce=$2+1 \
               AND NOT EXISTS (SELECT 1 FROM guardian_broadcast_jobs \
                               WHERE chain_id=$1 AND nonce>$2 AND state<>'abandoned')",
        )
        .bind(job.chain_id)
        .bind(job.nonce)
        .execute(&mut *db)
        .await?;
        let proposal_state: String =
            sqlx::query_scalar("SELECT state FROM guardian_proposals WHERE id=$1 FOR UPDATE")
                .bind(&job.proposal_id)
                .fetch_one(&mut *db)
                .await?;
        if matches!(proposal_state.as_str(), "approved" | "auto_approved") {
            expire_proposal(
                &mut db,
                &job.proposal_id,
                &proposal_state,
                &job.grant_id,
                "approval_expired_before_broadcast",
            )
            .await?;
        }
        db.commit().await?;
        Ok(())
    }

    async fn defer_job(&self, proposal_id: &str, code: &str) -> Result<(), WorkerError> {
        sqlx::query(
            "UPDATE guardian_broadcast_jobs SET attempts=attempts+1,last_attempt_ts=now(), \
             next_attempt_ts=now() + make_interval(secs => LEAST(30, power(2, LEAST(attempts,4))::int)), \
             last_error_code=$2,updated_ts=now() \
             WHERE proposal_id=$1 AND state IN ('prepared','submitted')",
        )
        .bind(proposal_id)
        .bind(code)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn rpc_for_chain(chain_id: u64) -> Option<RpcClient> {
    std::env::var(format!("AETHER_GUARDIAN__RPC_{chain_id}"))
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(RpcClient::new)
}

async fn transition_to_broadcast(
    db: &mut Transaction<'_, Postgres>,
    job: &Job,
) -> Result<(), WorkerError> {
    let state: String =
        sqlx::query_scalar("SELECT state FROM guardian_proposals WHERE id=$1 FOR UPDATE")
            .bind(&job.proposal_id)
            .fetch_one(&mut **db)
            .await?;
    if matches!(state.as_str(), "approved" | "auto_approved") {
        sqlx::query(
            "UPDATE guardian_proposals SET state='broadcast',tx_hash=$2,updated_ts=now() \
             WHERE id=$1",
        )
        .bind(&job.proposal_id)
        .bind(&job.tx_hash)
        .execute(&mut **db)
        .await?;
        insert_worker_event(
            db,
            &job.proposal_id,
            Some(&state),
            "broadcast",
            &job.grant_id,
            "raw_transaction_submitted",
        )
        .await?;
    }
    Ok(())
}

async fn expire_proposal(
    db: &mut Transaction<'_, Postgres>,
    proposal_id: &str,
    from_state: &str,
    grant_id: &str,
    reason: &str,
) -> Result<(), WorkerError> {
    sqlx::query("UPDATE guardian_proposals SET state='expired',updated_ts=now() WHERE id=$1")
        .bind(proposal_id)
        .execute(&mut **db)
        .await?;
    insert_worker_event(db, proposal_id, Some(from_state), "expired", grant_id, reason).await
}

async fn insert_worker_event(
    db: &mut Transaction<'_, Postgres>,
    proposal_id: &str,
    from_state: Option<&str>,
    to_state: &str,
    grant_id: &str,
    reason: &str,
) -> Result<(), WorkerError> {
    sqlx::query(
        "INSERT INTO guardian_proposal_events \
         (id,proposal_id,from_state,to_state,actor_id,grant_id,reason) \
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(Ulid::new().to_string())
    .bind(proposal_id)
    .bind(from_state)
    .bind(to_state)
    .bind(WORKER_ACTOR)
    .bind(grant_id)
    .bind(reason)
    .execute(&mut **db)
    .await?;
    Ok(())
}
