pub mod manifest;
pub mod registry;
pub mod sandbox;
pub mod signing;

pub use manifest::{Capability, PluginKind, PluginManifest};
pub use registry::PluginRegistry;
pub use sandbox::SandboxConfig;
pub use signing::{sign_manifest, verify_manifest, EdSignature as PluginSignature, KeyPair};
