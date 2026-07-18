//! Regression test infrastructure.
//!
//! Captures execution outputs as JSON goldens, computes aggregate SHA-256
//! fingerprints, and compares against stored goldens to detect regressions
//! across versions.
//!
//! # Golden format
//!
//! Each golden is stored as a JSON file containing an array of output records.
//! The file name convention is `<suite_name>/<case_name>.golden.json`.
//! An accompanying `<suite_name>/_manifest.json` tracks all cases and their
//! aggregate fingerprints.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// An individual regression case: named, versioned, carrying a set of outputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionCase {
    /// Unique name within its suite.
    pub name: String,
    /// Semantic version or build identifier.
    pub version: String,
    /// The output records (e.g. serialized order intents, fills, etc.).
    pub outputs: Vec<serde_json::Value>,
    /// SHA-256 hex digest of the concatenated canonical JSON outputs.
    pub fingerprint: String,
}

/// The result of comparing a case against its stored golden.
#[derive(Debug, Clone)]
pub struct RegressionResult {
    pub case_name: String,
    pub passed: bool,
    pub golden_fingerprint: String,
    pub actual_fingerprint: String,
    /// Descriptions of each mismatch (empty when `passed` is `true`).
    pub diffs: Vec<String>,
}

/// Error conditions during golden I/O and comparison.
#[derive(Debug, thiserror::Error)]
pub enum RegressionError {
    #[error("serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("golden not found: {0}")]
    GoldenNotFound(String),
    #[error("manifest not found: {0}")]
    ManifestNotFound(String),
    #[error("fingerprint mismatch: {0}")]
    Fingerprint(String),
    #[error("manifest version mismatch: expected {expected} got {actual}")]
    ManifestVersionMismatch { expected: String, actual: String },
}

/// Manifest file structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    suite_name: String,
    version: String,
    cases: HashMap<String, String>, // case_name -> fingerprint
}

// ---------------------------------------------------------------------------
// RegressionSuite
// ---------------------------------------------------------------------------

/// A named collection of regression cases stored under a golden directory.
///
/// # Storage layout
///
/// ```text
/// <golden_dir>/
///   <suite_name>/
///     _manifest.json
///     <case_name>.golden.json
///     ...
/// ```
pub struct RegressionSuite {
    suite_name: String,
    version: String,
    golden_dir: PathBuf,
    cases: HashMap<String, RegressionCase>,
}

impl RegressionSuite {
    /// Create or load a regression suite backed by `golden_dir`.
    ///
    /// If the suite already exists on disk the in-memory cases are populated
    /// from the manifest and golden files.
    pub fn new(
        suite_name: impl Into<String>,
        version: impl Into<String>,
        golden_dir: impl Into<PathBuf>,
    ) -> Result<Self, RegressionError> {
        let suite_name = suite_name.into();
        let version = version.into();
        let golden_dir: PathBuf = golden_dir.into();
        let suite_path = golden_dir.join(&suite_name);

        let cases = if suite_path.exists() {
            Self::load_manifest(&suite_path, &suite_name, &version)?
        } else {
            fs::create_dir_all(&suite_path)?;
            HashMap::new()
        };

        Ok(Self { suite_name, version, golden_dir, cases })
    }

    /// Register a new regression case with outputs.
    ///
    /// Computes the fingerprint and stores the case in memory.
    /// Does **not** write to disk until `persist_golden()` is called.
    pub fn add_case(&mut self, name: impl Into<String>, outputs: Vec<serde_json::Value>) {
        let name = name.into();
        let outputs_bytes: Vec<u8> = outputs
            .iter()
            .flat_map(|v| {
                let mut b = serde_json::to_vec(v).unwrap_or_default();
                b.push(b'\n'); // newline separator for canonical form
                b
            })
            .collect();
        let fingerprint = hex_sha256(&outputs_bytes);

        let case = RegressionCase {
            name: name.clone(),
            version: self.version.clone(),
            outputs,
            fingerprint,
        };
        self.cases.insert(name, case);
    }

    /// Persist the current golden files to disk.
    ///
    /// Writes one file per case plus a manifest.
    pub fn persist_golden(&self) -> Result<(), RegressionError> {
        let suite_path = self.golden_dir.join(&self.suite_name);
        fs::create_dir_all(&suite_path)?;

        let mut manifest_cases = HashMap::new();

        for (name, case) in &self.cases {
            let file_path = suite_path.join(format!("{name}.golden.json"));
            let json = serde_json::to_string_pretty(&case)?;
            fs::write(&file_path, json)?;
            manifest_cases.insert(name.clone(), case.fingerprint.clone());
        }

        let manifest = Manifest {
            suite_name: self.suite_name.clone(),
            version: self.version.clone(),
            cases: manifest_cases,
        };
        let manifest_path = suite_path.join("_manifest.json");
        let manifest_json = serde_json::to_string_pretty(&manifest)?;
        fs::write(manifest_path, manifest_json)?;

        Ok(())
    }

    /// Load a saved case from disk.
    pub fn load_case(&self, name: &str) -> Result<RegressionCase, RegressionError> {
        let suite_path = self.golden_dir.join(&self.suite_name);
        let file_path = suite_path.join(format!("{name}.golden.json"));
        if !file_path.exists() {
            return Err(RegressionError::GoldenNotFound(name.to_string()));
        }
        let bytes = fs::read(&file_path)?;
        let case: RegressionCase = serde_json::from_slice(&bytes)?;
        Ok(case)
    }

    /// Compare current in-memory outputs against stored goldens.
    ///
    /// Returns a vector of `RegressionResult` — one per case that differs
    /// (skips cases where fingerprints match).
    pub fn compare_all(&self) -> Result<Vec<RegressionResult>, RegressionError> {
        let mut results = Vec::new();

        for (name, case) in &self.cases {
            let golden = self.load_case(name)?;
            let passed = golden.fingerprint == case.fingerprint;
            let mut diffs = Vec::new();

            if !passed {
                if golden.outputs.len() != case.outputs.len() {
                    diffs.push(format!(
                        "output count mismatch: golden={} actual={}",
                        golden.outputs.len(),
                        case.outputs.len()
                    ));
                }
                for (i, (g, a)) in golden.outputs.iter().zip(case.outputs.iter()).enumerate() {
                    if g != a {
                        diffs.push(format!("output[{}] differs: golden={} actual={}", i, g, a));
                    }
                }
            }

            results.push(RegressionResult {
                case_name: name.clone(),
                passed,
                golden_fingerprint: golden.fingerprint,
                actual_fingerprint: case.fingerprint.clone(),
                diffs,
            });
        }

        Ok(results)
    }

    /// Get a reference to a specific case.
    pub fn get_case(&self, name: &str) -> Option<&RegressionCase> {
        self.cases.get(name)
    }

    /// Return the number of registered cases.
    pub fn len(&self) -> usize {
        self.cases.len()
    }

    /// Returns `true` if no cases are registered.
    pub fn is_empty(&self) -> bool {
        self.cases.is_empty()
    }

    /// Compute a fingerprint for a sequence of JSON values.
    pub fn fingerprint(outputs: &[serde_json::Value]) -> String {
        let mut all_bytes = Vec::new();
        for v in outputs {
            if let Ok(b) = serde_json::to_vec(v) {
                all_bytes.extend(b);
                all_bytes.push(b'\n');
            }
        }
        hex_sha256(&all_bytes)
    }

    // ── private helpers ─────────────────────────────────────────────

    fn load_manifest(
        suite_path: &Path,
        expected_suite_name: &str,
        expected_version: &str,
    ) -> Result<HashMap<String, RegressionCase>, RegressionError> {
        let manifest_path = suite_path.join("_manifest.json");
        if !manifest_path.exists() {
            return Err(RegressionError::ManifestNotFound(manifest_path.display().to_string()));
        }
        let bytes = fs::read(&manifest_path)?;
        let manifest: Manifest = serde_json::from_slice(&bytes)?;

        if manifest.suite_name != expected_suite_name {
            return Err(RegressionError::ManifestVersionMismatch {
                expected: format!("suite={expected_suite_name}"),
                actual: format!("suite={}", manifest.suite_name),
            });
        }
        if manifest.version != expected_version {
            return Err(RegressionError::ManifestVersionMismatch {
                expected: format!("version={expected_version}"),
                actual: format!("version={}", manifest.version),
            });
        }

        let mut cases = HashMap::new();
        for name in manifest.cases.keys() {
            let file_path = suite_path.join(format!("{name}.golden.json"));
            if !file_path.exists() {
                return Err(RegressionError::GoldenNotFound(name.to_string()));
            }
            let case_bytes = fs::read(&file_path)?;
            let case: RegressionCase = serde_json::from_slice(&case_bytes)?;
            cases.insert(name.clone(), case);
        }
        Ok(cases)
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect::<String>()
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_outputs(values: &[serde_json::Value]) -> Vec<serde_json::Value> {
        values.to_vec()
    }

    #[test]
    fn add_case_and_persist_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let mut suite = RegressionSuite::new("test_suite", "1.0.0", dir.path()).unwrap();

        let outputs = make_outputs(&[serde_json::json!({"id": 1, "val": "a"})]);
        suite.add_case("case_a", outputs);
        suite.persist_golden().unwrap();

        // Reload from disk
        let loaded = RegressionSuite::new("test_suite", "1.0.0", dir.path()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(loaded.get_case("case_a").is_some());
    }

    #[test]
    fn compare_matching_cases_passes() {
        let dir = tempfile::tempdir().unwrap();
        let mut suite = RegressionSuite::new("cmp", "1.0.0", dir.path()).unwrap();

        let outputs = make_outputs(&[serde_json::json!({"order": "abc", "status": "filled"})]);
        suite.add_case("match", outputs.clone());
        suite.persist_golden().unwrap();

        // Re-open and add same case — should match
        let mut suite2 = RegressionSuite::new("cmp", "1.0.0", dir.path()).unwrap();
        suite2.add_case("match", outputs);
        let results = suite2.compare_all().unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].passed);
    }

    #[test]
    fn compare_different_cases_detects_regression() {
        let dir = tempfile::tempdir().unwrap();
        let mut suite = RegressionSuite::new("regr", "1.0.0", dir.path()).unwrap();

        let original = make_outputs(&[serde_json::json!({"value": 100})]);
        suite.add_case("golden", original);
        suite.persist_golden().unwrap();

        // Load golden and add different output
        let mut suite2 = RegressionSuite::new("regr", "1.0.0", dir.path()).unwrap();
        let changed = make_outputs(&[serde_json::json!({"value": 200})]);
        suite2.add_case("golden", changed);
        let results = suite2.compare_all().unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].passed);
        assert!(!results[0].diffs.is_empty());
    }

    #[test]
    fn fingerprint_is_deterministic() {
        let outputs = make_outputs(&[serde_json::json!({"a": 1, "b": 2})]);

        let fp1 = RegressionSuite::fingerprint(&outputs);
        let fp2 = RegressionSuite::fingerprint(&outputs);
        assert_eq!(fp1, fp2);
        assert_eq!(fp1.len(), 64);
    }

    #[test]
    fn fingerprint_differs_on_content_change() {
        let outputs_a = make_outputs(&[serde_json::json!({"x": 1})]);
        let outputs_b = make_outputs(&[serde_json::json!({"x": 2})]);

        let fp_a = RegressionSuite::fingerprint(&outputs_a);
        let fp_b = RegressionSuite::fingerprint(&outputs_b);
        assert_ne!(fp_a, fp_b);
    }

    #[test]
    fn empty_suite_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let suite = RegressionSuite::new("empty", "0.0.1", dir.path()).unwrap();
        assert!(suite.is_empty());
        assert_eq!(suite.len(), 0);
    }

    #[test]
    fn load_nonexistent_golden_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let suite = RegressionSuite::new("err", "1.0.0", dir.path()).unwrap();

        let result = suite.load_case("nonexistent");
        match result {
            Err(RegressionError::GoldenNotFound(_)) => {}
            other => panic!("expected GoldenNotFound, got: {other:?}"),
        }
    }
}
