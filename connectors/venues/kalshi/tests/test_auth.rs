//! Integration tests for Kalshi authentication.
//!
//! Tests verify signature format, environment variable handling, and
//! key-derivation rejection cases.

use aether_venue_kalshi::KalshiAuth;

/// A test PKCS#8 RSA private key (2048-bit, for testing only).
const TEST_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQDJJgEmkCH8nR55
pqhp/MIFR4hIr/dvbhrY+Ja3VM+qnq9vUD0lvPkPSdvwMVT05n6YVtMMM3ionLcA
bjSX2qjMBQozVih7xZonMKCLryJehbZNLGzPZD4aOv2P8PtctY/pNisa7tG73OvC
OXdlefIz+jMoiHNVNzl/HoVH0HxR4YHASe4lDaPtbbAciw60mpC2G8XWGJFmZGYj
WYfSmZ5tt3nqyQSOZpgzD4TiVXMOGRtjJIk0FdHd1sgo/dDIn6uKoH9j4qV3Mfr8
Z1alWAmt+Pfkwkw6Tcx2Jwtvhh6WNaEXk5+9UyEw+D+U5DqdMOPuD7fFL/y9icDC
fJ+Y9zgfAgMBAAECggEABJkDybfdrwKAYdN3YgTPAoPiD5dGFpvzrSXxe/tKS+IY
rHivDR/GqZzMlC7sfDSQjDbf2BWNGn2KiU37kcUDurYax5Wek0WvAlpQMSEtre9s
fVMYoZzu9naGuTWO6U2VHoWIcrMmxB6GnQfnPMCO0rVTWgfUaww6Gje+YCfZz51H
iJNNrLS9qFiWwO/DbEIOIyKRmAwF+h62Tfc7UQG2HIkJtMagRvCos2/+/gcDJ183
Tnno5XisuJ1B3LVvzh1BNqZaWKiXJZmZA5vpz2cFlaKGFE/IVgzgvKxvDrEt2d7D
j5uYVUb+6oft7BIZem2jkQQLQKez1ZMRmNSXa83BAQKBgQD5O1dcPfvVSczN4/6X
NzrjgkLQ5nNP57PM+gS1LGIVXztFywEQjftTF0R9tKFFqi6rVq5VO5Zjg7P1BiGc
Rk7rZy8mQnZo54MT2JYTpVhX9gUYXwSEOnc9sFyx+ncBPmKkwTvSwZhVdkhCEggw
CZI3VZgpJB0damAWhajQOcOa3wKBgQDOnGNRTYkHiD5Cr76l+CpKONqKiNUaKiZx
ZehBsKMCAfv9z77i/H/Wsbgn/HxDinmIBskF71fAKUOOGcBusJ4cGJRh2B6vBy6g
hm+b+2nSawgWaF7+ttRfVzFGH+nETHClzRaHc3h0p2ccnSxwVu6nW1p1jyMx0c/q
gtynUKVKwQKBgDpVPEY3r7ilFE1gPpdP8vWK6G6ScYzTM08XeYCaCb7s0iessuwX
/ynceUhevZxbj57Eo/sI/lL+YWFI9RbpkdEhDnUK+0HkZdaAS+f/PCUiTOD+ZEU6
lewXWirB75aX7miXXZQfgbMHAzSLmeT8aH+RBhMjA7l9y02aLP/HdVPLAoGAJyR5
rG2ECGlHYlrpQ4hAes9Kl/RUayCRJ+qmlcthFoBJvUweXeJ4VbRVrz2mTSVu4NZo
PzeY6E7o/YLjchUD307IzcCkD4TM0JyniGWZJsQgRB6B4L/CfE2IiECDiSzyKncw
TXkS2QbeAg3E3YOasxobiSoVANs/CK7CHvCoYAECgYEA7+emQFZmbSrWlhn7xeEy
OMQVeC/F6xKe4lGiuXsnjKEO1K6bi3qvltRoUdhH7bnR+k55hbDZG1sRZpl+N5VV
L/pwyKxACFxRoBxJqeozXdOqWB/2nw+byZNtK1KfQLnAyGqADXPnXPBUxVFE+c/2
8jqtMyHz94du+Z7Y/kOyNns=
-----END PRIVATE KEY-----";

#[test]
fn sign_request_produces_valid_format() {
    let auth = KalshiAuth::from_pem_bytes("test-key-abc", TEST_KEY_PEM.as_bytes())
        .expect("valid PEM should construct KalshiAuth");

    let sig = auth.sign_request("GET", "/trade-api/v2/markets", "1712345678000").unwrap();

    // Must be non-empty
    assert!(!sig.is_empty(), "signature should not be empty");

    // Must be valid Base64
    assert!(
        sig.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '='),
        "signature contains non-base64 characters: {sig}"
    );
}

#[test]
fn pss_signatures_use_random_salt() {
    let auth = KalshiAuth::from_pem_bytes("k", TEST_KEY_PEM.as_bytes())
        .expect("valid PEM should construct KalshiAuth");

    let sig1 = auth.sign_request("GET", "/markets", "1000").unwrap();
    let sig2 = auth.sign_request("GET", "/markets", "1000").unwrap();

    assert_ne!(sig1, sig2, "RSA-PSS signatures should use random salt");
}

#[test]
fn sign_request_produces_different_results_for_different_messages() {
    let auth = KalshiAuth::from_pem_bytes("k", TEST_KEY_PEM.as_bytes())
        .expect("valid PEM should construct KalshiAuth");

    let sig1 = auth.sign_request("GET", "/markets", "1000").unwrap();
    let sig2 = auth.sign_request("POST", "/markets", "1000").unwrap();

    assert_ne!(sig1, sig2, "different messages should produce different signatures");
}

#[test]
fn key_and_signature_are_separate_header_values() {
    let auth = KalshiAuth::from_pem_bytes("key_id_123", TEST_KEY_PEM.as_bytes())
        .expect("valid PEM should construct KalshiAuth");

    let sig = auth.sign_request("GET", "/v2/markets", "1712345678000").unwrap();
    assert_eq!(auth.key_id(), "key_id_123");
    assert!(!sig.is_empty());
}

#[test]
fn from_pem_bytes_rejects_wrong_label() {
    let wrong_pem = "-----BEGIN RSA PRIVATE KEY-----\nMIIEvA==\n-----END RSA PRIVATE KEY-----";
    let result = KalshiAuth::from_pem_bytes("x", wrong_pem.as_bytes());
    assert!(result.is_err(), "should reject non-PKCS8 PEM label");
}

#[test]
fn from_pem_bytes_rejects_bogus_bytes() {
    let result = KalshiAuth::from_pem_bytes("x", b"not a pem file at all");
    assert!(result.is_err(), "should reject non-PEM bytes");
}

#[test]
fn from_pem_bytes_rejects_truncated_key() {
    let result = KalshiAuth::from_pem_bytes("x", &TEST_KEY_PEM.as_bytes()[..50]);
    assert!(result.is_err(), "should reject truncated PEM");
}

#[test]
fn key_id_is_accessible() {
    let auth = KalshiAuth::from_pem_bytes("custom-key-name", TEST_KEY_PEM.as_bytes())
        .expect("valid PEM should construct KalshiAuth");

    assert_eq!(auth.key_id(), "custom-key-name");
}

#[test]
fn multiple_auth_instances_are_independent() {
    let auth1 = KalshiAuth::from_pem_bytes("key1", TEST_KEY_PEM.as_bytes()).unwrap();
    let auth2 = KalshiAuth::from_pem_bytes("key2", TEST_KEY_PEM.as_bytes()).unwrap();

    assert_eq!(auth1.key_id(), "key1");
    assert_eq!(auth2.key_id(), "key2");

    assert!(!auth1.sign_request("GET", "/markets", "1000").unwrap().is_empty());
    assert!(!auth2.sign_request("GET", "/markets", "1000").unwrap().is_empty());
}
