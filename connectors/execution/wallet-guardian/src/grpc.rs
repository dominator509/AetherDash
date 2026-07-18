//! Durable gRPC boundary for the Wallet Guardian.

// tonic::Status is intentionally the error vocabulary at this trust boundary.
#![allow(clippy::result_large_err)]

use crate::keystore::KeyStore;
use crate::policy::allowlist::AllowList;
use crate::policy::{simulate_async, PolicyConfig, PolicyEngine};
use crate::proposal::{proposal_hash, CustodyMode, TxSpec};
use crate::totp::TotpAuthority;
use aether_authz::hash_session_token;
use aether_core::ids::Ulid;
use aether_proto::aether::guardian::v1::wallet_guardian_server::WalletGuardian;
use aether_proto::aether::guardian::v1::{
    ApproveProposalRequest, Proposal as WireProposal, ProposalRequest,
    ProposalStatus as WireStatus, TxSpec as WireTxSpec,
};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::str::FromStr;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use zeroize::Zeroize;

const GUARDIAN_APPROVAL_SCOPE: &str = "guardian.approve";
const GUARDIAN_PROPOSE_SCOPE: &str = "guardian.transfer";
const ALLOWLIST_COOLDOWN_HOURS: i64 = 24;

#[derive(Clone)]
pub struct GuardianGrpc {
    pool: PgPool,
    keystore: Arc<KeyStore>,
    totp: Arc<dyn TotpAuthority>,
    config: RuntimeConfig,
}

#[derive(Clone)]
struct RuntimeConfig {
    service_token_hash: Option<String>,
    allowed_destinations: Vec<String>,
    allowed_contract_calls: Vec<(String, String)>,
    policy: PolicyConfig,
    price_max_age: Duration,
}

#[derive(Clone)]
struct Principal {
    actor_id: String,
    actor_kind: String,
    grant_id: String,
    tier: u8,
    session_id: Option<String>,
    totp_secret_ref: Option<String>,
}

impl RuntimeConfig {
    fn from_env() -> Result<Self, Status> {
        let now = Utc::now();
        let allowed_destinations = parse_activated_destinations(
            &std::env::var("AETHER_GUARDIAN__ALLOWED_DESTINATIONS").unwrap_or_default(),
            now,
        )?;
        let service_token_hash = std::env::var("AETHER_GUARDIAN__SERVICE_TOKEN")
            .ok()
            .filter(|value| !value.is_empty())
            .map(|mut value| {
                let hash = hex::encode(Sha256::digest(value.as_bytes()));
                value.zeroize();
                hash
            });
        let allowed_contract_calls = parse_activated_contract_calls(
            &std::env::var("AETHER_GUARDIAN__ALLOWED_CONTRACT_CALLS").unwrap_or_default(),
            now,
        )?;
        Ok(Self {
            service_token_hash,
            allowed_destinations,
            allowed_contract_calls,
            policy: PolicyConfig::default(),
            price_max_age: Duration::seconds(60),
        })
    }
}

fn parse_activated_destinations(value: &str, now: DateTime<Utc>) -> Result<Vec<String>, Status> {
    value
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let (address, activated_at) = entry.rsplit_once('@').ok_or_else(|| {
                Status::invalid_argument("Guardian destination allowlist activation is missing")
            })?;
            validate_address(address)?;
            require_allowlist_cooldown(activated_at, now)?;
            Ok(address.to_lowercase())
        })
        .collect()
}

fn parse_activated_contract_calls(
    value: &str,
    now: DateTime<Utc>,
) -> Result<Vec<(String, String)>, Status> {
    value
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let (call, activated_at) = entry.rsplit_once('@').ok_or_else(|| {
                Status::invalid_argument("Guardian contract allowlist activation is missing")
            })?;
            let (address, selector) = call.split_once(':').ok_or_else(|| {
                Status::invalid_argument("Guardian contract allowlist entry is invalid")
            })?;
            validate_address(address)?;
            if selector.len() != 10
                || !selector.starts_with("0x")
                || !selector[2..].bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(Status::invalid_argument("Guardian contract selector is invalid"));
            }
            require_allowlist_cooldown(activated_at, now)?;
            Ok((address.to_lowercase(), selector.to_lowercase()))
        })
        .collect()
}

fn require_allowlist_cooldown(value: &str, now: DateTime<Utc>) -> Result<(), Status> {
    let activated_at = DateTime::parse_from_rfc3339(value)
        .map_err(|_| Status::invalid_argument("Guardian allowlist activation time is invalid"))?
        .with_timezone(&Utc);
    if activated_at > now
        || now.signed_duration_since(activated_at) < Duration::hours(ALLOWLIST_COOLDOWN_HOURS)
    {
        return Err(Status::failed_precondition(
            "Guardian allowlist entry is still in its 24-hour cooldown",
        ));
    }
    Ok(())
}

impl GuardianGrpc {
    pub fn from_env(
        pool: PgPool,
        keystore: KeyStore,
        totp: Arc<dyn TotpAuthority>,
    ) -> Result<Self, Status> {
        Self::from_env_shared(pool, Arc::new(keystore), totp)
    }

    pub fn from_env_shared(
        pool: PgPool,
        keystore: Arc<KeyStore>,
        totp: Arc<dyn TotpAuthority>,
    ) -> Result<Self, Status> {
        Ok(Self { pool, keystore, totp, config: RuntimeConfig::from_env()? })
    }

    #[cfg(test)]
    fn scope_allowed(scopes: &Value, required: &str) -> bool {
        scope_allowed(scopes, required)
    }

    async fn authenticate<T>(&self, request: &Request<T>) -> Result<Principal, Status> {
        let authorization = request
            .metadata()
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("missing bearer authentication"))?;
        let token = authorization
            .strip_prefix("Bearer ")
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Status::unauthenticated("invalid bearer authentication"))?;
        let token_hash = hex::encode(Sha256::digest(token.as_bytes()));

        if self
            .config
            .service_token_hash
            .as_deref()
            .is_some_and(|expected| constant_time_equal(expected.as_bytes(), token_hash.as_bytes()))
        {
            let actor_id = metadata_text(request, "x-aether-actor-id")?;
            let actor_kind = metadata_text(request, "x-aether-actor-kind")?;
            if !matches!(actor_kind.as_str(), "human" | "agent" | "automation") {
                return Err(Status::unauthenticated("invalid service actor kind"));
            }
            return self.current_grant(actor_id, actor_kind, None, None, None).await;
        }

        let session_hash = hash_session_token(token);
        let row = sqlx::query(
            "SELECT s.id, s.user_id, s.tier, s.origin_kind, u.totp_secret_ref \
             FROM sessions s JOIN users u ON u.id=s.user_id \
             WHERE s.token_hash=$1 AND s.expires_ts>now() \
               AND s.idle_expires_ts>now() AND s.revoked_ts IS NULL",
        )
        .bind(session_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| Status::unauthenticated("session is invalid or expired"))?;
        let actor_id: String = row.get("user_id");
        let actor_kind: String = row.get("origin_kind");
        if actor_kind != "human" {
            return Err(Status::unauthenticated("approval sessions must be human"));
        }
        self.current_grant(
            actor_id,
            actor_kind,
            Some(row.get("tier")),
            Some(row.get("id")),
            row.try_get("totp_secret_ref").ok(),
        )
        .await
    }

    async fn current_grant(
        &self,
        actor_id: String,
        actor_kind: String,
        session_tier: Option<i32>,
        session_id: Option<String>,
        totp_secret_ref: Option<String>,
    ) -> Result<Principal, Status> {
        let row = sqlx::query(
            "SELECT id, tier, scopes FROM permission_grants \
             WHERE actor_id=$1 AND actor_kind=$2 AND revoked_ts IS NULL \
               AND (expires_ts IS NULL OR expires_ts>now()) \
             ORDER BY tier DESC, expires_ts DESC NULLS FIRST, id ASC LIMIT 1",
        )
        .bind(&actor_id)
        .bind(&actor_kind)
        .fetch_optional(&self.pool)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| Status::permission_denied("no current permission grant"))?;
        let grant_tier: i32 = row.get("tier");
        let effective = session_tier.map_or(grant_tier, |tier| tier.min(grant_tier));
        let tier = u8::try_from(effective)
            .map_err(|_| Status::permission_denied("permission tier is invalid"))?;
        Ok(Principal {
            actor_id,
            actor_kind,
            grant_id: row.get("id"),
            tier,
            session_id,
            totp_secret_ref,
        })
    }

    fn require_scope(
        &self,
        principal: &Principal,
        scopes: &Value,
        scope: &str,
        minimum_tier: u8,
    ) -> Result<(), Status> {
        if principal.tier < minimum_tier {
            return Err(Status::permission_denied("permission tier is insufficient"));
        }
        if !scope_allowed(scopes, scope) {
            return Err(Status::permission_denied("permission grant scope is insufficient"));
        }
        Ok(())
    }

    async fn grant_scopes(&self, grant_id: &str) -> Result<Value, Status> {
        sqlx::query_scalar("SELECT scopes FROM permission_grants WHERE id=$1")
            .bind(grant_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(unavailable)?
            .ok_or_else(|| Status::permission_denied("permission grant disappeared"))
    }

    async fn revalidate_principal(
        &self,
        db: &mut Transaction<'_, Postgres>,
        principal: &Principal,
        scope: &str,
        minimum_tier: u8,
        require_human_session: bool,
    ) -> Result<(), Status> {
        let grant = sqlx::query(
            "SELECT tier, scopes FROM permission_grants \
             WHERE id=$1 AND actor_id=$2 AND actor_kind=$3 \
               AND revoked_ts IS NULL AND (expires_ts IS NULL OR expires_ts>now()) \
             FOR SHARE",
        )
        .bind(&principal.grant_id)
        .bind(&principal.actor_id)
        .bind(&principal.actor_kind)
        .fetch_optional(&mut **db)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| Status::permission_denied("permission grant is no longer current"))?;
        let grant_tier: i32 = grant.get("tier");
        let effective_tier = if let Some(session_id) = principal.session_id.as_deref() {
            let session_tier: i32 = sqlx::query_scalar(
                "SELECT tier FROM sessions WHERE id=$1 AND user_id=$2 \
                 AND origin_kind='human' AND expires_ts>now() AND idle_expires_ts>now() \
                 AND revoked_ts IS NULL FOR SHARE",
            )
            .bind(session_id)
            .bind(&principal.actor_id)
            .fetch_optional(&mut **db)
            .await
            .map_err(unavailable)?
            .ok_or_else(|| Status::unauthenticated("human session is no longer current"))?;
            session_tier.min(grant_tier)
        } else {
            if require_human_session {
                return Err(Status::unauthenticated("human session is required"));
            }
            grant_tier
        };
        if effective_tier < i32::from(minimum_tier) || !scope_allowed(&grant.get("scopes"), scope) {
            return Err(Status::permission_denied("current grant cannot perform this action"));
        }
        Ok(())
    }

    async fn durable_usage(
        db: &mut Transaction<'_, Postgres>,
        destination: &str,
    ) -> Result<(Decimal, Decimal), Status> {
        let row = sqlx::query(
            "SELECT COALESCE(sum(value_delta_usd),0)::text AS daily, \
                    COALESCE(sum(value_delta_usd) FILTER \
                      (WHERE lower(tx_spec->>'to')=lower($1)),0)::text AS destination \
             FROM guardian_proposals \
             WHERE state IN ('pending','approved','auto_approved','broadcast','confirmed') \
               AND updated_ts>now()-INTERVAL '24 hours'",
        )
        .bind(destination)
        .fetch_one(&mut **db)
        .await
        .map_err(unavailable)?;
        Ok((parse_decimal(row.get("daily"))?, parse_decimal(row.get("destination"))?))
    }

    async fn reference_asset(&self, asset_id: &str) -> Result<(Decimal, u32), Status> {
        let row = sqlx::query(
            "SELECT price_usd::text AS price, asset_decimals, observed_ts \
             FROM guardian_reference_prices WHERE asset_id=$1 \
             ORDER BY observed_ts DESC, id DESC LIMIT 1",
        )
        .bind(asset_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| Status::failed_precondition("reference price is unavailable"))?;
        let observed_at: DateTime<Utc> = row.get("observed_ts");
        if observed_at > Utc::now()
            || Utc::now().signed_duration_since(observed_at) > self.config.price_max_age
        {
            return Err(Status::failed_precondition("reference price is stale"));
        }
        let decimals: Option<i16> = row.get("asset_decimals");
        let decimals = decimals
            .ok_or_else(|| Status::failed_precondition("reference asset precision is unavailable"))
            .and_then(|value| {
                u32::try_from(value).map_err(|_| {
                    Status::failed_precondition("reference asset precision is invalid")
                })
            })?;
        Ok((parse_decimal(row.get("price"))?, decimals))
    }
}

#[tonic::async_trait]
impl WalletGuardian for GuardianGrpc {
    async fn propose_transaction(
        &self,
        request: Request<WireTxSpec>,
    ) -> Result<Response<WireProposal>, Status> {
        if !self.keystore.is_available() {
            return Err(Status::unavailable("Guardian keystore is unavailable"));
        }
        let principal = self.authenticate(&request).await?;
        let scopes = self.grant_scopes(&principal.grant_id).await?;
        self.require_scope(&principal, &scopes, GUARDIAN_PROPOSE_SCOPE, 4)?;
        let wire = request.into_inner();
        let (tx, custody, asset_id, asset_decimals) = parse_wire_tx(wire)?;
        let expected_asset_id = if tx.data.eq_ignore_ascii_case("0x") {
            format!("eip155:{}/native", tx.chain_id)
        } else {
            format!("eip155:{}/erc20:{}", tx.chain_id, tx.to.to_lowercase())
        };
        if asset_id != expected_asset_id {
            return Err(Status::invalid_argument("asset_id does not match transaction"));
        }
        let (asset_price, authoritative_decimals) = self.reference_asset(&asset_id).await?;
        if asset_decimals != authoritative_decimals {
            return Err(Status::invalid_argument(
                "asset_decimals does not match the trusted reference asset",
            ));
        }
        let value_delta = requested_value_usd(&tx, asset_price, authoritative_decimals)?;
        let rpc_var = format!("AETHER_GUARDIAN__RPC_{}", tx.chain_id);
        let rpc_url = std::env::var(&rpc_var)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| Status::unavailable("operator-configured chain RPC is unavailable"))?;
        let simulation = simulate_async(&tx, tx.chain_id, value_delta, Some(&rpc_url)).await;
        if simulation.error.as_deref().is_some_and(|error| error.starts_with("RPC simulation")) {
            return Err(Status::unavailable("chain simulation is unavailable"));
        }
        let destination_refs =
            self.config.allowed_destinations.iter().map(String::as_str).collect();
        let call_refs = self
            .config
            .allowed_contract_calls
            .iter()
            .map(|(address, selector)| (address.as_str(), selector.as_str()))
            .collect();
        let mut engine = PolicyEngine {
            allowlist: AllowList::new()
                .with_allowed_destinations(destination_refs)
                .with_allowed_contract_calls(call_refs),
            ..PolicyEngine::new(self.config.policy.clone())
        };
        let withdrawal = value_delta > Decimal::ZERO || !tx.data.eq_ignore_ascii_case("0x");
        let id = Ulid::new().to_string();
        let hash = proposal_hash(&tx, custody);
        let tx_json = serde_json::to_value(&tx)
            .map_err(|_| Status::internal("transaction could not be serialized"))?;
        let custody_text = custody_text(custody);
        let mut db = self.pool.begin().await.map_err(unavailable)?;
        self.revalidate_principal(&mut db, &principal, GUARDIAN_PROPOSE_SCOPE, 4, false).await?;
        // Limit checks and the proposal that reserves that exposure are one
        // serialized transaction. Without this lock, concurrent proposals can
        // all observe the same usage and collectively exceed a hard ceiling.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('guardian-limit-budget', 0))")
            .execute(&mut *db)
            .await
            .map_err(unavailable)?;
        let (daily, destination) = Self::durable_usage(&mut db, &tx.to).await?;
        engine.limits.seed_usage(daily, &tx.to, destination);
        let verdict = engine.evaluate_with_simulation(&tx, simulation, withdrawal, principal.tier);
        let state = if !verdict.allowed {
            "denied"
        } else if verdict.requires_human {
            "pending"
        } else {
            "auto_approved"
        };
        let trace = serde_json::to_value(&verdict.trace)
            .map_err(|_| Status::internal("policy trace could not be serialized"))?;
        sqlx::query(
            "INSERT INTO guardian_proposals \
             (id, proposer_actor_id, proposer_actor_kind, grant_id, tx_spec, custody_mode, \
              state, policy_trace, proposal_hash, value_delta_usd, approved_at, \
              approval_expires_at, expires_ts) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::numeric, \
                     CASE WHEN $7='auto_approved' THEN now() END, \
                     CASE WHEN $7='auto_approved' THEN now()+INTERVAL '60 seconds' END, \
                     now()+INTERVAL '10 minutes')",
        )
        .bind(&id)
        .bind(&principal.actor_id)
        .bind(&principal.actor_kind)
        .bind(&principal.grant_id)
        .bind(tx_json)
        .bind(custody_text)
        .bind(state)
        .bind(&trace)
        .bind(&hash)
        .bind(value_delta.to_string())
        .execute(&mut *db)
        .await
        .map_err(unavailable)?;
        insert_event(&mut db, &id, None, state, &principal, "policy_evaluated").await?;
        db.commit().await.map_err(unavailable)?;
        Ok(Response::new(wire_proposal(&id, state, trace, &hash)))
    }

    async fn get_proposal(
        &self,
        request: Request<ProposalRequest>,
    ) -> Result<Response<WireProposal>, Status> {
        let principal = self.authenticate(&request).await?;
        let scopes = self.grant_scopes(&principal.grant_id).await?;
        if principal.actor_kind == "human" {
            self.require_scope(&principal, &scopes, GUARDIAN_APPROVAL_SCOPE, 4)?;
        }
        let id = request.into_inner().id;
        validate_ulid(&id)?;
        let mut db = self.pool.begin().await.map_err(unavailable)?;
        self.revalidate_principal(
            &mut db,
            &principal,
            if principal.actor_kind == "human" {
                GUARDIAN_APPROVAL_SCOPE
            } else {
                GUARDIAN_PROPOSE_SCOPE
            },
            4,
            principal.actor_kind == "human",
        )
        .await?;
        let proposal = sqlx::query(
            "SELECT p.id, p.state, p.policy_trace, p.proposal_hash, p.expires_ts, \
                    p.approval_expires_at, j.state AS broadcast_job_state \
             FROM guardian_proposals p \
             LEFT JOIN guardian_broadcast_jobs j ON j.proposal_id=p.id \
             WHERE p.id=$1 FOR UPDATE OF p",
        )
        .bind(&id)
        .fetch_optional(&mut *db)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| Status::not_found("proposal was not found"))?;
        let mut state: String = proposal.get("state");
        let pending_expiry: DateTime<Utc> = proposal.get("expires_ts");
        let approval_expiry: Option<DateTime<Utc>> = proposal.get("approval_expires_at");
        let broadcast_job_state: Option<String> = proposal.try_get("broadcast_job_state").ok();
        let stale = match state.as_str() {
            "pending" => pending_expiry <= Utc::now(),
            "approved" | "auto_approved" => match approval_expiry {
                Some(expiry) => {
                    expiry <= Utc::now()
                        && !matches!(broadcast_job_state.as_deref(), Some("prepared" | "submitted"))
                }
                None => true,
            },
            _ => false,
        };
        if stale {
            let previous = state.clone();
            sqlx::query(
                "UPDATE guardian_proposals SET state='expired', updated_ts=now() WHERE id=$1",
            )
            .bind(&id)
            .execute(&mut *db)
            .await
            .map_err(unavailable)?;
            insert_event(&mut db, &id, Some(&previous), "expired", &principal, "proposal_expired")
                .await?;
            state = "expired".into();
        }
        db.commit().await.map_err(unavailable)?;
        Ok(Response::new(wire_proposal(
            &proposal.get::<String, _>("id"),
            &state,
            proposal.get("policy_trace"),
            &proposal.get::<String, _>("proposal_hash"),
        )))
    }

    async fn approve_proposal(
        &self,
        request: Request<ApproveProposalRequest>,
    ) -> Result<Response<WireProposal>, Status> {
        if !self.keystore.is_available() {
            return Err(Status::unavailable("Guardian keystore is unavailable"));
        }
        let principal = self.authenticate(&request).await?;
        if principal.actor_kind != "human" || principal.session_id.is_none() {
            return Err(Status::unauthenticated("human session is required"));
        }
        let scopes = self.grant_scopes(&principal.grant_id).await?;
        self.require_scope(&principal, &scopes, GUARDIAN_APPROVAL_SCOPE, 4)?;
        let body = request.into_inner();
        validate_ulid(&body.id)?;
        let approval = body
            .approval
            .ok_or_else(|| Status::failed_precondition("approval proof is required"))?;
        if approval.reference.len() < 32 || approval.totp.len() != 6 {
            return Err(Status::failed_precondition("approval proof is invalid"));
        }
        let reference_hash = hex::encode(Sha256::digest(approval.reference.as_bytes()));
        let now = Utc::now();
        let now_seconds = u64::try_from(now.timestamp())
            .map_err(|_| Status::internal("system clock is invalid"))?;
        let mut db = self.pool.begin().await.map_err(unavailable)?;
        self.revalidate_principal(&mut db, &principal, GUARDIAN_APPROVAL_SCOPE, 4, true).await?;
        let proposal = sqlx::query(
            "SELECT state, policy_trace, proposal_hash, expires_ts, tx_spec, custody_mode \
             FROM guardian_proposals WHERE id=$1 FOR UPDATE",
        )
        .bind(&body.id)
        .fetch_optional(&mut *db)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| Status::not_found("proposal was not found"))?;
        let state: String = proposal.get("state");
        let expires_at: DateTime<Utc> = proposal.get("expires_ts");
        let stored_hash: String = proposal.get("proposal_hash");
        let stored_tx: TxSpec = serde_json::from_value(proposal.get("tx_spec"))
            .map_err(|_| Status::failed_precondition("stored proposal payload is invalid"))?;
        let stored_custody = parse_custody(proposal.get::<String, _>("custody_mode").as_str())?;
        let recomputed_hash = proposal_hash(&stored_tx, stored_custody);
        if state != "pending" {
            return Err(Status::failed_precondition("proposal is not pending"));
        }
        if expires_at <= now {
            expire_locked(&mut db, &body.id, &principal).await?;
            db.commit().await.map_err(unavailable)?;
            return Err(Status::failed_precondition("proposal is expired"));
        }
        if !constant_time_equal(stored_hash.as_bytes(), recomputed_hash.as_bytes())
            || !constant_time_equal(
                stored_hash.as_bytes(),
                approval.expected_proposal_hash.as_bytes(),
            )
        {
            return Err(Status::failed_precondition("proposal hash binding failed"));
        }
        let reference = sqlx::query(
            "SELECT id, actor_id, action, target_id, requires_step_up, status, expires_ts \
             FROM approval_references WHERE token_hash=$1 FOR UPDATE",
        )
        .bind(&reference_hash)
        .fetch_optional(&mut *db)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| Status::failed_precondition("approval reference is invalid"))?;
        let reference_id: String = reference.get("id");
        if reference.get::<String, _>("actor_id") != principal.actor_id
            || reference.get::<String, _>("action") != "guardian"
            || reference.get::<String, _>("target_id") != body.id
            || !reference.get::<bool, _>("requires_step_up")
            || reference.get::<String, _>("status") != "pending"
            || reference.get::<DateTime<Utc>, _>("expires_ts") <= now
        {
            insert_approval_attempt(
                &mut db,
                Some(&reference_id),
                &principal.actor_id,
                "denied",
                "reference_binding_failed",
            )
            .await?;
            db.commit().await.map_err(unavailable)?;
            return Err(Status::failed_precondition("approval reference binding failed"));
        }
        let challenge = sqlx::query(
            "SELECT id, actor_id, action, target_id, expires_ts, consumed_ts \
             FROM step_up_challenges \
             WHERE token_hash=$1 AND approval_reference_id=$2 FOR UPDATE",
        )
        .bind(&reference_hash)
        .bind(&reference_id)
        .fetch_optional(&mut *db)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| Status::failed_precondition("step-up challenge is missing"))?;
        let challenge_id: String = challenge.get("id");
        if challenge.get::<String, _>("actor_id") != principal.actor_id
            || challenge.get::<String, _>("action") != "guardian_approval"
            || challenge.get::<Option<String>, _>("target_id").as_deref() != Some(&body.id)
            || challenge.get::<DateTime<Utc>, _>("expires_ts") <= now
            || challenge.get::<Option<DateTime<Utc>>, _>("consumed_ts").is_some()
        {
            insert_approval_attempt(
                &mut db,
                Some(&reference_id),
                &principal.actor_id,
                "denied",
                "step_up_binding_failed",
            )
            .await?;
            db.commit().await.map_err(unavailable)?;
            return Err(Status::failed_precondition("step-up challenge binding failed"));
        }
        let Some(secret_ref) = principal.totp_secret_ref.as_deref() else {
            insert_approval_attempt(
                &mut db,
                Some(&reference_id),
                &principal.actor_id,
                "denied",
                "totp_not_enrolled",
            )
            .await?;
            db.commit().await.map_err(unavailable)?;
            return Err(Status::failed_precondition("operator TOTP is not enrolled"));
        };
        let verified = match self.totp.verify(secret_ref, &approval.totp, now_seconds) {
            Ok(verified) => verified,
            Err(_) => {
                insert_approval_attempt(
                    &mut db,
                    Some(&reference_id),
                    &principal.actor_id,
                    "denied",
                    "totp_verifier_unavailable",
                )
                .await?;
                db.commit().await.map_err(unavailable)?;
                return Err(Status::unavailable("TOTP verifier is unavailable"));
            }
        };
        if !verified {
            let prior_invalid_attempts: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM approval_attempts \
                 WHERE approval_id=$1 AND actor_id=$2 AND reason='totp_invalid'",
            )
            .bind(&reference_id)
            .bind(&principal.actor_id)
            .fetch_one(&mut *db)
            .await
            .map_err(unavailable)?;
            let attempt_limit_reached = prior_invalid_attempts >= 4;
            if attempt_limit_reached {
                sqlx::query(
                    "UPDATE step_up_challenges SET consumed_ts=now(), session_id=$2 \
                     WHERE id=$1 AND consumed_ts IS NULL",
                )
                .bind(&challenge_id)
                .bind(principal.session_id.as_deref())
                .execute(&mut *db)
                .await
                .map_err(unavailable)?;
                sqlx::query(
                    "UPDATE approval_references SET status='failed', consumed_ts=now(), \
                     updated_ts=now() WHERE id=$1 AND status='pending'",
                )
                .bind(&reference_id)
                .execute(&mut *db)
                .await
                .map_err(unavailable)?;
            }
            insert_approval_attempt(
                &mut db,
                Some(&reference_id),
                &principal.actor_id,
                "denied",
                if attempt_limit_reached { "totp_attempt_limit" } else { "totp_invalid" },
            )
            .await?;
            db.commit().await.map_err(unavailable)?;
            return Err(Status::failed_precondition(if attempt_limit_reached {
                "approval challenge failed after too many TOTP attempts"
            } else {
                "fresh TOTP verification failed"
            }));
        }
        sqlx::query(
            "UPDATE step_up_challenges SET consumed_ts=now(), session_id=$2 \
             WHERE id=$1 AND consumed_ts IS NULL",
        )
        .bind(&challenge_id)
        .bind(principal.session_id.as_deref())
        .execute(&mut *db)
        .await
        .map_err(unavailable)?;
        sqlx::query(
            "UPDATE approval_references \
             SET status='approved', consumed_ts=now(), updated_ts=now() WHERE id=$1",
        )
        .bind(&reference_id)
        .execute(&mut *db)
        .await
        .map_err(unavailable)?;
        sqlx::query(
            "UPDATE guardian_proposals SET state='approved', approved_at=now(), \
             approval_expires_at=now()+INTERVAL '60 seconds', updated_ts=now() WHERE id=$1",
        )
        .bind(&body.id)
        .execute(&mut *db)
        .await
        .map_err(unavailable)?;
        insert_approval_attempt(
            &mut db,
            Some(&reference_id),
            &principal.actor_id,
            "approved",
            "guardian_approved",
        )
        .await?;
        insert_event(&mut db, &body.id, Some("pending"), "approved", &principal, "human_step_up")
            .await?;
        db.commit().await.map_err(unavailable)?;
        Ok(Response::new(wire_proposal(
            &body.id,
            "approved",
            proposal.get("policy_trace"),
            &stored_hash,
        )))
    }
}

async fn insert_event(
    db: &mut Transaction<'_, Postgres>,
    proposal_id: &str,
    from_state: Option<&str>,
    to_state: &str,
    principal: &Principal,
    reason: &str,
) -> Result<(), Status> {
    sqlx::query(
        "INSERT INTO guardian_proposal_events \
         (id, proposal_id, from_state, to_state, actor_id, grant_id, reason) \
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(Ulid::new().to_string())
    .bind(proposal_id)
    .bind(from_state)
    .bind(to_state)
    .bind(&principal.actor_id)
    .bind(&principal.grant_id)
    .bind(reason)
    .execute(&mut **db)
    .await
    .map_err(unavailable)?;
    Ok(())
}

async fn insert_approval_attempt(
    db: &mut Transaction<'_, Postgres>,
    approval_id: Option<&str>,
    actor_id: &str,
    outcome: &str,
    reason: &str,
) -> Result<(), Status> {
    sqlx::query(
        "INSERT INTO approval_attempts \
         (id, approval_id, actor_id, channel, decision, outcome, reason) \
         VALUES ($1,$2,$3,'client','approve',$4,$5)",
    )
    .bind(Ulid::new().to_string())
    .bind(approval_id)
    .bind(actor_id)
    .bind(outcome)
    .bind(reason)
    .execute(&mut **db)
    .await
    .map_err(unavailable)?;
    Ok(())
}

async fn expire_locked(
    db: &mut Transaction<'_, Postgres>,
    proposal_id: &str,
    principal: &Principal,
) -> Result<(), Status> {
    sqlx::query("UPDATE guardian_proposals SET state='expired', updated_ts=now() WHERE id=$1")
        .bind(proposal_id)
        .execute(&mut **db)
        .await
        .map_err(unavailable)?;
    insert_event(db, proposal_id, Some("pending"), "expired", principal, "approval_expired").await
}

fn parse_wire_tx(wire: WireTxSpec) -> Result<(TxSpec, CustodyMode, String, u32), Status> {
    let chain_id = wire
        .chain_id
        .parse::<u64>()
        .map_err(|_| Status::invalid_argument("chain_id is invalid"))?;
    if chain_id == 0 {
        return Err(Status::invalid_argument("chain_id is invalid"));
    }
    let custody = match wire.custody_mode.as_str() {
        "guardian_custody" => CustodyMode::GuardianCustody,
        "wallet_connect" => CustodyMode::WalletConnect,
        _ => return Err(Status::invalid_argument("custody_mode is invalid")),
    };
    if wire.asset_id.is_empty() || wire.asset_id.len() > 160 {
        return Err(Status::invalid_argument("asset_id is invalid"));
    }
    validate_address(&wire.to)?;
    validate_hex_quantity(&wire.value, "value")?;
    let max_fee = validate_hex_quantity(&wire.max_fee_per_gas, "max_fee_per_gas")?;
    let priority_fee =
        validate_hex_quantity(&wire.max_priority_fee_per_gas, "max_priority_fee_per_gas")?;
    if priority_fee > max_fee {
        return Err(Status::invalid_argument("max_priority_fee_per_gas exceeds max_fee_per_gas"));
    }
    if wire.gas_limit == 0 {
        return Err(Status::invalid_argument("gas_limit must be positive"));
    }
    validate_calldata(&wire.data)?;
    let asset_decimals = wire.asset_decimals;
    if asset_decimals > 28 {
        return Err(Status::invalid_argument("asset_decimals exceeds supported precision"));
    }
    Ok((
        TxSpec {
            chain_id,
            to: wire.to,
            value: wire.value,
            data: wire.data,
            gas_limit: wire.gas_limit,
            max_fee_per_gas: wire.max_fee_per_gas,
            max_priority_fee_per_gas: wire.max_priority_fee_per_gas,
        },
        custody,
        wire.asset_id,
        asset_decimals,
    ))
}

fn validate_address(value: &str) -> Result<(), Status> {
    if value.len() != 42
        || !value.starts_with("0x")
        || !value[2..].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(Status::invalid_argument("transaction destination is invalid"));
    }
    Ok(())
}

fn validate_hex_quantity(value: &str, field: &'static str) -> Result<u128, Status> {
    let digits = value
        .strip_prefix("0x")
        .filter(|digits| {
            !digits.is_empty()
                && (digits.len() == 1 || !digits.starts_with('0'))
                && digits.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        .ok_or_else(|| Status::invalid_argument(format!("{field} is invalid")))?;
    u128::from_str_radix(digits, 16)
        .map_err(|_| Status::invalid_argument(format!("{field} exceeds supported precision")))
}

fn validate_calldata(value: &str) -> Result<(), Status> {
    const MAX_CALLDATA_HEX_CHARS: usize = 262_144;
    let digits = value
        .strip_prefix("0x")
        .ok_or_else(|| Status::invalid_argument("transaction data is invalid"))?;
    if digits.len() > MAX_CALLDATA_HEX_CHARS
        || digits.len() % 2 != 0
        || !digits.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(Status::invalid_argument("transaction data is invalid"));
    }
    Ok(())
}

fn parse_decimal(value: &str) -> Result<Decimal, Status> {
    Decimal::from_str(value).map_err(|_| Status::invalid_argument("decimal value is invalid"))
}

fn parse_hex_u128(value: &str) -> Result<u128, Status> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if value.is_empty() {
        return Ok(0);
    }
    u128::from_str_radix(value, 16)
        .map_err(|_| Status::invalid_argument("transaction value is invalid"))
}

fn native_value_usd(value: &str, asset_price: Decimal) -> Result<Decimal, Status> {
    scaled_value_usd(parse_hex_u128(value)?, 18, asset_price)
}

fn requested_value_usd(
    tx: &TxSpec,
    asset_price: Decimal,
    asset_decimals: u32,
) -> Result<Decimal, Status> {
    if tx.data.eq_ignore_ascii_case("0x") {
        return native_value_usd(&tx.value, asset_price);
    }
    let calldata = tx.data.strip_prefix("0x").unwrap_or(&tx.data);
    let (amount_start, amount_end) = match calldata.get(..8).map(str::to_ascii_lowercase).as_deref()
    {
        Some("a9059cbb" | "095ea7b3") => (72, 136),
        Some("23b872dd") => (136, 200),
        _ => {
            return Err(Status::failed_precondition(
                "contract balance-delta derivation is unavailable",
            ))
        }
    };
    let amount = calldata
        .get(amount_start..amount_end)
        .ok_or_else(|| Status::invalid_argument("token calldata is truncated"))?;
    scaled_value_usd(parse_hex_u128(amount)?, asset_decimals, asset_price)
}

fn scaled_value_usd(amount: u128, decimals: u32, asset_price: Decimal) -> Result<Decimal, Status> {
    let amount = Decimal::from_str(&amount.to_string())
        .map_err(|_| Status::invalid_argument("asset amount exceeds supported precision"))?;
    let mut scale = Decimal::ONE;
    for _ in 0..decimals {
        scale = scale
            .checked_mul(Decimal::TEN)
            .ok_or_else(|| Status::invalid_argument("asset decimals exceed supported precision"))?;
    }
    amount
        .checked_div(scale)
        .and_then(|units| units.checked_mul(asset_price))
        .ok_or_else(|| Status::invalid_argument("asset value exceeds supported precision"))
}

fn custody_text(custody: CustodyMode) -> &'static str {
    match custody {
        CustodyMode::GuardianCustody => "guardian_custody",
        CustodyMode::WalletConnect => "wallet_connect",
    }
}

fn parse_custody(value: &str) -> Result<CustodyMode, Status> {
    match value {
        "guardian_custody" => Ok(CustodyMode::GuardianCustody),
        "wallet_connect" => Ok(CustodyMode::WalletConnect),
        _ => Err(Status::failed_precondition("stored custody mode is invalid")),
    }
}

fn wire_proposal(id: &str, state: &str, trace: Value, hash: &str) -> WireProposal {
    WireProposal {
        id: id.to_owned(),
        status: wire_status(state) as i32,
        policy_trace: serde_json::to_string(&trace).unwrap_or_else(|_| "[]".into()),
        proposal_hash: hash.to_owned(),
    }
}

fn wire_status(state: &str) -> WireStatus {
    match state {
        "pending" => WireStatus::Pending,
        "auto_approved" => WireStatus::AutoApproved,
        "denied" => WireStatus::Denied,
        "approved" => WireStatus::Approved,
        "expired" => WireStatus::Expired,
        "broadcast" => WireStatus::Broadcast,
        "confirmed" => WireStatus::Confirmed,
        "failed" => WireStatus::Failed,
        _ => WireStatus::Unspecified,
    }
}

fn metadata_text<T>(request: &Request<T>, key: &'static str) -> Result<String, Status> {
    request
        .metadata()
        .get(key)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| Status::unauthenticated("service actor metadata is missing"))
}

fn validate_ulid(value: &str) -> Result<(), Status> {
    Ulid::from_string(value)
        .map(|_| ())
        .map_err(|_| Status::invalid_argument("identifier is invalid"))
}

fn scope_allowed(scopes: &Value, required: &str) -> bool {
    let allowed = if let Some(array) = scopes.as_array() {
        Some(array)
    } else {
        scopes.get("allowed").and_then(Value::as_array)
    };
    match allowed {
        None => scopes.as_object().is_some_and(|object| object.is_empty()),
        Some(values) => values.iter().any(|value| value.as_str() == Some(required)),
    }
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter().zip(right).fold(0_u8, |difference, (a, b)| difference | (a ^ b)) == 0
}

fn unavailable(error: sqlx::Error) -> Status {
    let _ = error;
    Status::unavailable("Guardian persistence is unavailable")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_scope_object_is_unrestricted_but_explicit_empty_list_denies() {
        assert!(GuardianGrpc::scope_allowed(&serde_json::json!({}), GUARDIAN_APPROVAL_SCOPE));
        assert!(!GuardianGrpc::scope_allowed(
            &serde_json::json!({"allowed": []}),
            GUARDIAN_APPROVAL_SCOPE
        ));
    }

    #[test]
    fn constant_time_comparison_rejects_mismatch() {
        assert!(constant_time_equal(b"same", b"same"));
        assert!(!constant_time_equal(b"same", b"diff"));
        assert!(!constant_time_equal(b"same", b"short"));
    }

    #[test]
    fn wire_surface_has_no_arbitrary_signing_fields() {
        let request = ApproveProposalRequest {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
            approval: Some(aether_proto::aether::guardian::v1::Approval {
                totp: "000000".into(),
                ts: "2026-07-17T00:00:00Z".into(),
                reference: "opaque".into(),
                expected_proposal_hash: "00".repeat(32),
            }),
        };
        assert!(request.approval.is_some());
    }

    #[test]
    fn native_value_is_derived_from_transaction_wei_and_price() {
        assert_eq!(
            native_value_usd("0xde0b6b3a7640000", Decimal::new(2_000, 0)).unwrap(),
            Decimal::new(2_000, 0)
        );
    }

    #[test]
    fn erc20_transfer_value_is_derived_from_calldata() {
        let tx = TxSpec {
            chain_id: 1,
            to: "0x1234567890123456789012345678901234567890".into(),
            value: "0x0".into(),
            data: concat!(
                "0xa9059cbb",
                "0000000000000000000000001111111111111111111111111111111111111111",
                "00000000000000000000000000000000000000000000000000000000000f4240"
            )
            .into(),
            gas_limit: 50_000,
            max_fee_per_gas: "0x1".into(),
            max_priority_fee_per_gas: "0x1".into(),
        };
        assert_eq!(requested_value_usd(&tx, Decimal::new(2, 0), 6).unwrap(), Decimal::new(2, 0));
    }

    #[test]
    fn wire_transaction_rejects_malformed_signing_fields_before_policy() {
        let valid = || WireTxSpec {
            to: "0x1234567890123456789012345678901234567890".into(),
            value: "0x0".into(),
            data: "0x".into(),
            chain_id: "137".into(),
            gas_limit: 21_000,
            max_fee_per_gas: "0x2".into(),
            max_priority_fee_per_gas: "0x1".into(),
            custody_mode: "guardian_custody".into(),
            asset_id: "eip155:137/native".into(),
            asset_decimals: 18,
        };

        let mut malformed_fee = valid();
        malformed_fee.max_fee_per_gas = "garbage".into();
        assert!(parse_wire_tx(malformed_fee).is_err());

        let mut inverted_fees = valid();
        inverted_fees.max_priority_fee_per_gas = "0x3".into();
        assert!(parse_wire_tx(inverted_fees).is_err());

        let mut odd_calldata = valid();
        odd_calldata.data = "0x0".into();
        assert!(parse_wire_tx(odd_calldata).is_err());

        let mut zero_gas = valid();
        zero_gas.gas_limit = 0;
        assert!(parse_wire_tx(zero_gas).is_err());
    }

    #[test]
    fn allowlist_entries_require_a_completed_24_hour_cooldown() {
        let now = DateTime::parse_from_rfc3339("2026-07-18T12:00:00Z").unwrap().with_timezone(&Utc);
        let address = "0x1234567890123456789012345678901234567890";

        assert!(
            parse_activated_destinations(&format!("{address}@2026-07-17T11:59:59Z"), now).is_ok()
        );
        assert!(
            parse_activated_destinations(&format!("{address}@2026-07-17T12:00:01Z"), now).is_err()
        );
        assert!(parse_activated_destinations(address, now).is_err());
        assert!(parse_activated_contract_calls(
            &format!("{address}:0xa9059cbb@2026-07-17T11:59:59Z"),
            now
        )
        .is_ok());
    }
}
