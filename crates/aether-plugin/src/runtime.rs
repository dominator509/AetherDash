use crate::manifest::{Capability, PluginManifest};
use crate::sandbox::SandboxConfig;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use wasmi::{
    Caller, Config, EnforcedLimits, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder,
};

#[derive(Debug)]
struct HostState {
    granted: BTreeSet<Capability>,
    denied: Vec<Capability>,
    calls: Vec<Capability>,
    limits: StoreLimits,
}

impl HostState {
    fn invoke(&mut self, capability: Capability) -> i32 {
        self.calls.push(capability);
        if self.granted.contains(&capability) {
            1
        } else {
            self.denied.push(capability);
            0
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionReport {
    pub return_code: i32,
    pub host_calls: Vec<Capability>,
}

#[derive(Debug, Clone)]
pub struct WasmRuntime {
    engine: Engine,
}

impl WasmRuntime {
    #[must_use]
    pub fn new() -> Self {
        let mut config = Config::default();
        config.consume_fuel(true).enforced_limits(EnforcedLimits::strict());
        Self { engine: Engine::new(&config) }
    }

    pub fn execute(
        &self,
        manifest: &PluginManifest,
        wasm: &[u8],
        sandbox: &SandboxConfig,
    ) -> Result<ExecutionReport, RuntimeError> {
        let actual_hash = hex::encode(Sha256::digest(wasm));
        if actual_hash != manifest.wasm_hash {
            return Err(RuntimeError::WasmHashMismatch);
        }
        let requested: BTreeSet<_> = manifest.capabilities.iter().copied().collect();
        if !requested.is_subset(&sandbox.allowed_capabilities) {
            return Err(RuntimeError::OverScopedManifest);
        }
        if sandbox.allowed_capabilities.contains(&Capability::NetworkHttp) {
            return Err(RuntimeError::NetworkHostUnavailable);
        }

        let module = Module::new(&self.engine, wasm).map_err(|_| RuntimeError::InvalidModule)?;
        let limits = StoreLimitsBuilder::new()
            .memory_size(sandbox.max_memory_bytes)
            .instances(1)
            .memories(1)
            .tables(1)
            .build();
        let state = HostState {
            granted: sandbox.allowed_capabilities.clone(),
            denied: Vec::new(),
            calls: Vec::new(),
            limits,
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|state| &mut state.limits);
        store.set_fuel(sandbox.max_fuel).map_err(|_| RuntimeError::FuelConfiguration)?;
        let mut linker = Linker::new(&self.engine);
        define_host(&mut linker, "read_markets", Capability::ReadMarkets)?;
        define_host(&mut linker, "read_positions", Capability::ReadPositions)?;
        define_host(&mut linker, "submit_alert", Capability::SubmitAlerts)?;
        define_host(&mut linker, "access_brain", Capability::AccessBrain)?;
        define_host(&mut linker, "execute_paper", Capability::ExecutePaper)?;

        let instance = linker
            .instantiate(&mut store, &module)
            .and_then(|instance| instance.start(&mut store))
            .map_err(|_| RuntimeError::ImportOrStartDenied)?;
        let entry = instance
            .get_typed_func::<(), i32>(&store, &manifest.entry_point)
            .map_err(|_| RuntimeError::InvalidEntryPoint)?;
        let return_code = entry.call(&mut store, ()).map_err(|_| RuntimeError::ExecutionTrapped)?;
        if let Some(capability) = store.data().denied.first().copied() {
            return Err(RuntimeError::CapabilityDenied(capability));
        }
        Ok(ExecutionReport { return_code, host_calls: store.data().calls.clone() })
    }
}

impl Default for WasmRuntime {
    fn default() -> Self {
        Self::new()
    }
}

fn define_host(
    linker: &mut Linker<HostState>,
    name: &'static str,
    capability: Capability,
) -> Result<(), RuntimeError> {
    linker
        .func_wrap("aether", name, move |mut caller: Caller<'_, HostState>| -> i32 {
            caller.data_mut().invoke(capability)
        })
        .map_err(|_| RuntimeError::HostConfiguration)?;
    Ok(())
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RuntimeError {
    #[error("Wasm bytes do not match the signed manifest hash")]
    WasmHashMismatch,
    #[error("approved capabilities do not cover the signed manifest")]
    OverScopedManifest,
    #[error("network capability has no ambient socket host and requires a future proxy")]
    NetworkHostUnavailable,
    #[error("Wasm module is invalid or exceeds compilation limits")]
    InvalidModule,
    #[error("Wasm fuel could not be configured")]
    FuelConfiguration,
    #[error("Wasm import or start function was denied")]
    ImportOrStartDenied,
    #[error("manifest entry point must export () -> i32")]
    InvalidEntryPoint,
    #[error("Wasm execution trapped or exhausted fuel/memory")]
    ExecutionTrapped,
    #[error("host capability denied on invocation: {0:?}")]
    CapabilityDenied(Capability),
    #[error("capability host could not be configured")]
    HostConfiguration,
}
