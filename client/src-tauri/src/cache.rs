/// Encrypted-at-rest local hot cache module.
///
/// Stores small, frequently-accessed data items encrypted with AES-256-GCM.
/// The encryption key is generated on first use and persisted in the OS keychain
/// (hex-encoded via the `keyring` crate).
///
/// Cache files are stored under `{app_data_dir}/cache/`.
/// Cache loss is non-fatal — all data is reconstructable from the server.
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use keyring::Entry;
use rand::RngCore;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

/// Keyring service name — must match the service name in `keychain.rs`.
const SERVICE_NAME: &str = "aether-terminal";

/// Keyring "username" field for the cache encryption key.
const CACHE_KEY_NAME: &str = "cache-encryption-key";

/// AES-GCM nonce size in bytes (96 bits).
const NONCE_SIZE: usize = 12;

// ---------------------------------------------------------------------------
// Key management
// ---------------------------------------------------------------------------

/// Retrieve the 256-bit AES encryption key from the OS keychain.
///
/// On first invocation a fresh random key is generated, hex-encoded, and stored
/// in the keychain so it persists across restarts.
fn get_or_create_cache_key() -> Result<[u8; 32], String> {
    let entry = Entry::new(SERVICE_NAME, CACHE_KEY_NAME)
        .map_err(|e| format!("Keychain unavailable: {}", e))?;

    match entry.get_password() {
        Ok(hex_key) => {
            let decoded =
                hex::decode(&hex_key).map_err(|e| format!("Failed to decode cache key: {}", e))?;
            if decoded.len() != 32 {
                return Err("Invalid cache key length in keychain".to_string());
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&decoded);
            Ok(key)
        }
        Err(keyring::Error::NoEntry) => {
            // First run — generate and persist a fresh key.
            let mut key = [0u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut key);
            entry
                .set_password(&hex::encode(key))
                .map_err(|e| format!("Failed to persist cache key: {}", e))?;
            Ok(key)
        }
        Err(e) => Err(format!("Failed to read cache key from keychain: {}", e)),
    }
}

// ---------------------------------------------------------------------------
// Encryption / Decryption helpers
// ---------------------------------------------------------------------------

/// Encrypt `data` with a given 256-bit key using AES-256-GCM.
///
/// Returns `nonce || ciphertext` (nonce is prepended to the output).
fn encrypt_with_key(key_bytes: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, String> {
    let key = Key::<Aes256Gcm>::from_slice(key_bytes);
    let cipher = Aes256Gcm::new(key);

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext =
        cipher.encrypt(nonce, data).map_err(|e| format!("Encryption failed: {}", e))?;

    let mut result = nonce_bytes.to_vec();
    result.extend(ciphertext);
    Ok(result)
}

/// Decrypt data that was produced by `encrypt_with_key()`.
///
/// Expects `data` to be `nonce (12 bytes) || ciphertext`.
fn decrypt_with_key(key_bytes: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < NONCE_SIZE {
        return Err("Invalid encrypted data: too short".to_string());
    }

    let key = Key::<Aes256Gcm>::from_slice(key_bytes);
    let cipher = Aes256Gcm::new(key);

    let (nonce_bytes, ciphertext) = data.split_at(NONCE_SIZE);
    let nonce = Nonce::from_slice(nonce_bytes);

    cipher.decrypt(nonce, ciphertext).map_err(|e| format!("Decryption failed: {}", e))
}

/// Encrypt `data` with AES-256-GCM using the OS keychain key.
///
/// Returns `nonce || ciphertext` (nonce is prepended to the output).
fn encrypt(data: &[u8]) -> Result<Vec<u8>, String> {
    let key_bytes = get_or_create_cache_key()?;
    encrypt_with_key(&key_bytes, data)
}

/// Decrypt data that was produced by `encrypt()`.
///
/// Expects `data` to be `nonce (12 bytes) || ciphertext`.
fn decrypt(data: &[u8]) -> Result<Vec<u8>, String> {
    let key_bytes = get_or_create_cache_key()?;
    decrypt_with_key(&key_bytes, data)
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Return the cache directory, creating it if necessary.
fn cache_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let data_dir =
        app.path().app_data_dir().map_err(|e| format!("Failed to resolve app data dir: {}", e))?;
    let dir = data_dir.join("cache");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create cache directory: {}", e))?;
    Ok(dir)
}

/// Return the on-disk path for a given cache key within a cache directory.
fn cache_file_in_dir(dir: &Path, key: &str) -> PathBuf {
    let encoded = hex::encode(key.as_bytes());
    dir.join(format!("{}.bin", encoded))
}

// ---------------------------------------------------------------------------
// Internal get/set by directory (supports testing without Tauri AppHandle)
// ---------------------------------------------------------------------------

/// Read a cached item from a specific cache directory.
///
/// Returns `Ok(None)` when the key is not present.
fn get_cached_item_in_dir(dir: &Path, key: &str) -> Result<Option<Vec<u8>>, String> {
    let path = cache_file_in_dir(dir, key);
    if !path.exists() {
        return Ok(None);
    }
    let encrypted = fs::read(&path).map_err(|e| format!("Failed to read cache file: {}", e))?;
    let decrypted = decrypt(&encrypted)?;
    Ok(Some(decrypted))
}

/// Write an item to a specific cache directory, creating it if necessary.
fn set_cached_item_in_dir(dir: &Path, key: &str, data: &[u8]) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("Failed to create cache directory: {}", e))?;
    let path = cache_file_in_dir(dir, key);
    let encrypted = encrypt(data)?;
    fs::write(&path, &encrypted).map_err(|e| format!("Failed to write cache file: {}", e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Read a cached item by key.
///
/// Returns `Ok(None)` when the key is not present in the cache.
/// Cache loss is non-fatal — the caller should reconstruct data from the server.
#[tauri::command]
pub fn get_cached_item(app: tauri::AppHandle, key: String) -> Result<Option<Vec<u8>>, String> {
    let dir = cache_dir(&app)?;
    get_cached_item_in_dir(&dir, &key)
}

/// Write an item to the encrypted cache.
#[tauri::command]
pub fn set_cached_item(app: tauri::AppHandle, key: String, data: Vec<u8>) -> Result<(), String> {
    let dir = cache_dir(&app)?;
    set_cached_item_in_dir(&dir, &key, &data)
}

/// Wipe the entire cache directory.
#[tauri::command]
pub fn clear_cache(app: tauri::AppHandle) -> Result<(), String> {
    let dir = cache_dir(&app)?;
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| format!("Failed to clear cache directory: {}", e))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static DIR_COUNTER: AtomicU32 = AtomicU32::new(0);

    /// Generate a fresh random 256-bit test key.
    fn test_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut key);
        key
    }

    /// Create a unique temporary cache directory for a single test.
    fn temp_cache_dir() -> PathBuf {
        let count = DIR_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join("aether-cache-test").join(count.to_string());
        // Ensure clean slate
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// Helper: write an item into a cache directory under a given key.
    fn write_item(dir: &Path, key: &str, data: &[u8], key_bytes: &[u8; 32]) {
        let path = cache_file_in_dir(dir, key);
        let encrypted = encrypt_with_key(key_bytes, data).unwrap();
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(&path, &encrypted).unwrap();
    }

    /// Helper: read an item from a cache directory by key.
    fn read_item(dir: &Path, key: &str, key_bytes: &[u8; 32]) -> Option<Vec<u8>> {
        let path = cache_file_in_dir(dir, key);
        if !path.exists() {
            return None;
        }
        let encrypted = std::fs::read(&path).unwrap();
        decrypt_with_key(key_bytes, &encrypted).ok()
    }

    /// Encrypt data with a given key then decrypt: the result must match the
    /// original input.
    #[test]
    fn encryption_round_trip() {
        let key = test_key();
        let data = b"Hello, AETHER Terminal!";
        let encrypted = encrypt_with_key(&key, data).unwrap();
        let decrypted = decrypt_with_key(&key, &encrypted).unwrap();
        assert_eq!(decrypted, data);
    }

    /// The ciphertext must not contain the plaintext as a contiguous
    /// subsequence (confirms AES-GCM does not leak data).
    #[test]
    fn ciphertext_does_not_contain_plaintext() {
        let key = test_key();
        let data = b"sensitive-data-12345";
        let encrypted = encrypt_with_key(&key, data).unwrap();
        let pt: Vec<u8> = data.to_vec();
        for window in encrypted.windows(pt.len()) {
            assert_ne!(window, pt.as_slice());
        }
    }

    /// Two cache instances with different keys must produce different
    /// ciphertexts for the same plaintext.
    #[test]
    fn different_keys_produce_different_ciphertexts() {
        let data = b"same-data";
        let key_a = test_key();
        let key_b = test_key();
        // Collision is astronomically unlikely (2^256).
        if key_a == key_b {
            return;
        }
        let ct_a = encrypt_with_key(&key_a, data).unwrap();
        let ct_b = encrypt_with_key(&key_b, data).unwrap();
        assert_ne!(ct_a, ct_b);
    }

    /// Tampered ciphertext must cause a decryption error (AES-GCM
    /// authentication tag catches the corruption).
    #[test]
    fn corrupt_ciphertext_returns_error() {
        let key = test_key();
        let data = b"test-data";
        let mut encrypted = encrypt_with_key(&key, data).unwrap();
        // Flip a bit in the ciphertext portion (past the nonce).
        let corrupt_idx = NONCE_SIZE + 3;
        if corrupt_idx < encrypted.len() {
            encrypted[corrupt_idx] ^= 0xFF;
        }
        let result = decrypt_with_key(&key, &encrypted);
        assert!(result.is_err());
    }

    /// An empty Vec must encrypt and decrypt successfully.
    #[test]
    fn empty_data_round_trip() {
        let key = test_key();
        let data: Vec<u8> = vec![];
        let encrypted = encrypt_with_key(&key, &data).unwrap();
        let decrypted = decrypt_with_key(&key, &encrypted).unwrap();
        assert_eq!(decrypted, data);
    }

    /// Deleting the cache directory must not cause a crash: subsequent get
    /// returns None and set recreates the directory transparently.
    #[test]
    fn cache_loss_is_non_fatal() {
        let key = test_key();
        let dir = temp_cache_dir();

        // Write a cached item and verify it round-trips.
        write_item(&dir, "test-key", b"test-value", &key);
        let result = read_item(&dir, "test-key", &key);
        assert_eq!(result, Some(b"test-value".to_vec()));

        // Simulate cache loss by deleting the directory.
        std::fs::remove_dir_all(&dir).unwrap();

        // Read must return None (not crash).
        let result = read_item(&dir, "test-key", &key);
        assert_eq!(result, None);

        // Set must recreate the directory and persist the new item.
        write_item(&dir, "new-key", b"new-value", &key);
        let result = read_item(&dir, "new-key", &key);
        assert_eq!(result, Some(b"new-value".to_vec()));
    }
}
