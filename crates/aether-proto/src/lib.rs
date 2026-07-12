//! Compiled proto contracts for all SPEC-003 / EP-004 service packages.
//! Each proto package is re-exported under its canonical path so clients
//! and servers can reference types as `aether::brain::v1::ObjectDraft` etc.

pub mod aether {
    pub mod core {
        pub mod v1 {
            tonic::include_proto!("aether.core.v1");
        }
    }
    pub mod brain {
        pub mod v1 {
            tonic::include_proto!("aether.brain.v1");
        }
    }
    pub mod guardian {
        pub mod v1 {
            tonic::include_proto!("aether.guardian.v1");
        }
    }
    pub mod risk {
        pub mod v1 {
            tonic::include_proto!("aether.risk.v1");
        }
    }
    pub mod router {
        pub mod v1 {
            tonic::include_proto!("aether.router.v1");
        }
    }
    pub mod venue {
        pub mod v1 {
            tonic::include_proto!("aether.venue.v1");
        }
    }
}

#[cfg(test)]
mod tests {
    /// Compile-time smoke check: verify all 6 proto packages have expected types.
    #[test]
    fn proto_packages_compile() {
        // Core types
        let _m = crate::aether::core::v1::Money { amount: "1.00".into(), currency: "USD".into() };
        // Brain service
        let _d = crate::aether::brain::v1::ObjectDraft {
            kind: "test".into(),
            content: "".into(),
            source: "".into(),
        };
        // Guardian service
        let _t = crate::aether::guardian::v1::TxSpec {
            to: "".into(),
            value: "".into(),
            data: "".into(),
            chain_id: "".into(),
        };
        // Risk service
        // Risk uses only core types (OrderIntent, RiskVerdict) — no custom messages
        // Router service
        let _r = crate::aether::router::v1::RouterResult { order: None, verdict: None };
        // Venue service
        let _b = crate::aether::venue::v1::Balance {
            asset: "BTC".into(),
            free: "1.0".into(),
            locked: "0.0".into(),
        };
    }
}
