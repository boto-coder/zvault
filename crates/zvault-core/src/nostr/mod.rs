//! Nostr protocol integration for ZVault.
//!
//! ## Implemented NIPs
//!
//! - **NIP-01** — Event structure and Schnorr (BIP-340) signing.
//! - **NIP-44 v2** — Encrypted payloads: ECDH → HKDF → ChaCha20 + HMAC-SHA256.
//! - **NIP-59** — Gift-wrap: hide sender, recipient, and event kind from relays.
//!
//! ## Architecture
//!
//! - Secret keys live in [`crate::device::SecureStorage`]; this module accepts
//!   raw 32-byte secret key slices wrapped in [`zeroize::Zeroizing`].
//! - Relay communication (WebSocket pub/sub) lives in the [`crate::sync`] module.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chacha20::cipher::{KeyIvInit, StreamCipher};
use chacha20::ChaCha20;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use k256::{
    elliptic_curve::sec1::ToEncodedPoint, schnorr::SigningKey as SchnorrSigningKey, SecretKey,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use aes_gcm::aead::rand_core::RngCore as _;
use aes_gcm::aead::OsRng as AeadOsRng;

use crate::{Error, Result};

// ─── Constants ────────────────────────────────────────────────────────────────

/// NIP-44 v2 version byte.
const NIP44_VERSION: u8 = 2;

/// NIP-44 HKDF salt for conversation key derivation.
const NIP44_SALT: &[u8] = b"nip44-v2";

/// Minimum plaintext size (1 byte).
const MIN_PLAINTEXT_SIZE: usize = 1;

/// Maximum plaintext size (2^32 - 1).
const MAX_PLAINTEXT_SIZE: usize = 0xFFFF_FFFF;

/// Threshold for extended prefix (6 bytes instead of 2).
const EXTENDED_PREFIX_THRESHOLD: usize = 65536;

// ─── NostrEvent ────────────────────────────────────────────────────────────────

/// A NIP-01 Nostr event (signed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NostrEvent {
    /// Event ID: SHA-256 of the canonical serialisation (hex, 64 chars).
    pub id: String,
    /// Author public key (hex, 64 chars — x-only).
    pub pubkey: String,
    /// Unix timestamp (seconds).
    pub created_at: i64,
    /// Event kind number.
    pub kind: u32,
    /// Tag array (each tag is a string array).
    pub tags: Vec<Vec<String>>,
    /// Event content (ciphertext for vault sync events).
    pub content: String,
    /// Schnorr signature over the event ID (hex, 128 chars).
    pub sig: String,
}

// ─── NIP-01: Event Signing ────────────────────────────────────────────────────

/// Compute the NIP-01 event ID (SHA-256 of the canonical serialisation).
///
/// Canonical form: `[0, pubkey_hex, created_at, kind, tags, content]`
fn compute_event_id(
    pubkey: &str,
    created_at: i64,
    kind: u32,
    tags: &[Vec<String>],
    content: &str,
) -> String {
    let canonical = serde_json::to_string(&serde_json::json!([
        0, pubkey, created_at, kind, tags, content
    ]))
    .expect("invariant: canonical JSON serialisation cannot fail");

    let hash = Sha256::digest(canonical.as_bytes());
    hex::encode(hash)
}

/// Construct and sign a NIP-01 Nostr event.
///
/// `secret_key` is the 32-byte secp256k1 secret scalar (from SecureStorage).
///
/// # Errors
///
/// Returns [`Error::Crypto`] if the secret key is invalid or signing fails.
pub fn sign_event(
    secret_key: &Zeroizing<Vec<u8>>,
    content: &str,
    kind: u32,
    tags: Vec<Vec<String>>,
    created_at: i64,
) -> Result<NostrEvent> {
    use k256::schnorr::signature::hazmat::PrehashSigner;

    // Parse secret key.
    let sk = SecretKey::from_slice(secret_key)
        .map_err(|e| Error::Crypto(format!("invalid secret key: {e}")))?;

    // Derive x-only public key.
    let pubkey_hex = secret_key_to_pubkey_hex(&sk);

    // Compute event ID.
    let id = compute_event_id(&pubkey_hex, created_at, kind, &tags, content);

    // Sign with BIP-340 Schnorr.
    // The event ID is already SHA256(canonical_json), so we use sign_prehash
    // to avoid double-hashing (k256's `sign` applies SHA256 internally).
    let schnorr_key = SchnorrSigningKey::from(sk);
    let id_bytes =
        hex::decode(&id).map_err(|e| Error::Crypto(format!("hex decode event id: {e}")))?;
    let signature: k256::schnorr::Signature = schnorr_key
        .sign_prehash(&id_bytes)
        .map_err(|e| Error::Crypto(format!("schnorr sign failed: {e}")))?;
    let sig_hex = hex::encode(signature.to_bytes());

    Ok(NostrEvent {
        id,
        pubkey: pubkey_hex,
        created_at,
        kind,
        tags,
        content: content.to_string(),
        sig: sig_hex,
    })
}

/// Verify a NIP-01 event signature.
///
/// # Errors
///
/// Returns [`Error::Crypto`] if the signature or public key is invalid.
pub fn verify_event(event: &NostrEvent) -> Result<()> {
    use k256::schnorr::{signature::hazmat::PrehashVerifier, VerifyingKey};

    // Recompute event ID.
    let expected_id = compute_event_id(
        &event.pubkey,
        event.created_at,
        event.kind,
        &event.tags,
        &event.content,
    );
    if expected_id != event.id {
        return Err(Error::Crypto("event ID mismatch".into()));
    }

    // Parse public key.
    let pubkey_bytes =
        hex::decode(&event.pubkey).map_err(|e| Error::Crypto(format!("bad pubkey hex: {e}")))?;
    let vk = VerifyingKey::from_bytes(&pubkey_bytes)
        .map_err(|e| Error::Crypto(format!("invalid pubkey: {e}")))?;

    // Parse signature.
    let sig_bytes =
        hex::decode(&event.sig).map_err(|e| Error::Crypto(format!("bad sig hex: {e}")))?;
    let signature = k256::schnorr::Signature::try_from(sig_bytes.as_slice())
        .map_err(|e| Error::Crypto(format!("invalid signature: {e}")))?;

    // Verify.
    // Use verify_prehash because the event ID is already SHA256(canonical_json).
    let id_bytes =
        hex::decode(&event.id).map_err(|e| Error::Crypto(format!("bad event id hex: {e}")))?;
    vk.verify_prehash(&id_bytes, &signature)
        .map_err(|e| Error::Crypto(format!("signature verification failed: {e}")))?;

    Ok(())
}

// ─── NIP-44 v2: Encryption ────────────────────────────────────────────────────

/// Derive the NIP-44 conversation key from a private key and a public key.
///
/// `conversation_key = HKDF-extract(IKM=ECDH(priv, pub).x, salt="nip44-v2")`
///
/// # Errors
///
/// Returns [`Error::Crypto`] if the keys are invalid.
///
/// # Panics
///
/// This function will not panic — the internal `expect` is on `HMAC::new_from_slice`
/// which accepts any key size.
pub fn get_conversation_key(
    secret_key: &[u8],
    public_key_hex: &str,
) -> Result<Zeroizing<[u8; 32]>> {
    use k256::elliptic_curve::ecdh::diffie_hellman;

    // Parse our secret scalar.
    let sk = SecretKey::from_slice(secret_key)
        .map_err(|e| Error::Crypto(format!("invalid secret key: {e}")))?;

    // Parse recipient's public key (x-only → full point).
    let pub_bytes = hex::decode(public_key_hex)
        .map_err(|e| Error::Crypto(format!("invalid pubkey hex: {e}")))?;
    if pub_bytes.len() != 32 {
        return Err(Error::Crypto("pubkey must be 32 bytes (x-only)".into()));
    }

    // Reconstruct a full compressed point: prefix 0x02 + x-coordinate.
    let mut compressed = [0u8; 33];
    compressed[0] = 0x02;
    compressed[1..33].copy_from_slice(&pub_bytes);

    let public_key = k256::PublicKey::from_sec1_bytes(&compressed)
        .map_err(|e| Error::Crypto(format!("invalid public key point: {e}")))?;

    // ECDH: shared_x = (priv * Pub).x
    let shared_secret = diffie_hellman(sk.to_nonzero_scalar(), public_key.as_affine());
    let shared_x = shared_secret.raw_secret_bytes();

    // NIP-44 spec: conversation_key = HKDF-extract(IKM=shared_x, salt="nip44-v2")
    // HKDF-extract is defined as PRK = HMAC-Hash(salt, IKM).
    // The hkdf crate's `new()` performs extract internally but doesn't expose
    // the PRK directly in a convenient way, so we compute it manually.
    let mut conversation_key = Zeroizing::new([0u8; 32]);
    let mut hmac_extract =
        Hmac::<Sha256>::new_from_slice(NIP44_SALT).expect("HMAC accepts any key size");
    hmac_extract.update(shared_x);
    let prk = hmac_extract.finalize().into_bytes();
    conversation_key.copy_from_slice(&prk);

    Ok(conversation_key)
}

/// Derive per-message keys from conversation_key and nonce.
///
/// Returns `(chacha_key[32], chacha_nonce[12], hmac_key[32])`.
fn get_message_keys(
    conversation_key: &[u8; 32],
    nonce: &[u8; 32],
) -> Result<([u8; 32], [u8; 12], [u8; 32])> {
    // HKDF-expand(PRK=conversation_key, info=nonce, L=76)
    let hk = Hkdf::<Sha256>::from_prk(conversation_key)
        .map_err(|e| Error::Crypto(format!("HKDF from_prk failed: {e}")))?;

    let mut keys = [0u8; 76];
    hk.expand(nonce, &mut keys)
        .map_err(|e| Error::Crypto(format!("HKDF expand failed: {e}")))?;

    let mut chacha_key = [0u8; 32];
    let mut chacha_nonce = [0u8; 12];
    let mut hmac_key = [0u8; 32];

    chacha_key.copy_from_slice(&keys[0..32]);
    chacha_nonce.copy_from_slice(&keys[32..44]);
    hmac_key.copy_from_slice(&keys[44..76]);

    Ok((chacha_key, chacha_nonce, hmac_key))
}

/// Calculate the padded length for a given plaintext length (NIP-44 padding).
#[must_use]
pub fn calc_padded_len(unpadded_len: usize) -> usize {
    if unpadded_len <= 32 {
        return 32;
    }
    // next_power = 1 << (floor(log2(unpadded_len - 1)) + 1)
    let next_power = (unpadded_len - 1).next_power_of_two();
    let chunk = if next_power <= 256 {
        32
    } else {
        next_power / 8
    };
    chunk * ((unpadded_len - 1) / chunk + 1)
}

/// Pad plaintext per NIP-44 spec.
fn pad(plaintext: &[u8]) -> Result<Vec<u8>> {
    let unpadded_len = plaintext.len();
    if !(MIN_PLAINTEXT_SIZE..=MAX_PLAINTEXT_SIZE).contains(&unpadded_len) {
        return Err(Error::Crypto(format!(
            "plaintext length {unpadded_len} out of range"
        )));
    }

    let padded_len = calc_padded_len(unpadded_len);

    if unpadded_len >= EXTENDED_PREFIX_THRESHOLD {
        // 6-byte prefix: [0x00, 0x00] + u32 BE length
        let mut result = Vec::with_capacity(6 + padded_len);
        result.extend_from_slice(&[0u8; 2]);
        #[allow(clippy::cast_possible_truncation)] // validated by MAX_PLAINTEXT_SIZE (< u32::MAX)
        let len_u32 = unpadded_len as u32;
        result.extend_from_slice(&len_u32.to_be_bytes());
        result.extend_from_slice(plaintext);
        result.resize(6 + padded_len, 0);
        Ok(result)
    } else {
        // 2-byte prefix: u16 BE length
        let mut result = Vec::with_capacity(2 + padded_len);
        #[allow(clippy::cast_possible_truncation)] // guarded by EXTENDED_PREFIX_THRESHOLD check
        let len_u16 = unpadded_len as u16;
        result.extend_from_slice(&len_u16.to_be_bytes());
        result.extend_from_slice(plaintext);
        result.resize(2 + padded_len, 0);
        Ok(result)
    }
}

/// Remove NIP-44 padding and return the plaintext.
fn unpad(padded: &[u8]) -> Result<Vec<u8>> {
    if padded.len() < 2 {
        return Err(Error::Crypto("padded data too short".into()));
    }

    let first_two = u16::from_be_bytes([padded[0], padded[1]]);
    let (unpadded_len, prefix_len) = if first_two == 0 {
        // Extended format: next 4 bytes are u32 BE length.
        if padded.len() < 6 {
            return Err(Error::Crypto("extended prefix too short".into()));
        }
        let len = u32::from_be_bytes([padded[2], padded[3], padded[4], padded[5]]) as usize;
        if len < EXTENDED_PREFIX_THRESHOLD {
            return Err(Error::Crypto(
                "invalid padding: extended prefix for short message".into(),
            ));
        }
        (len, 6)
    } else {
        (first_two as usize, 2)
    };

    if unpadded_len == 0 {
        return Err(Error::Crypto("invalid padding: zero length".into()));
    }

    let expected_total = prefix_len + calc_padded_len(unpadded_len);
    if padded.len() != expected_total {
        return Err(Error::Crypto(format!(
            "invalid padding: expected {} bytes, got {}",
            expected_total,
            padded.len()
        )));
    }

    if prefix_len + unpadded_len > padded.len() {
        return Err(Error::Crypto("padding: plaintext overflows buffer".into()));
    }

    Ok(padded[prefix_len..prefix_len + unpadded_len].to_vec())
}

/// Compute HMAC-SHA256 with AAD (nonce prepended to message).
fn hmac_aad(key: &[u8; 32], message: &[u8], aad: &[u8; 32]) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts 32-byte key");
    mac.update(aad);
    mac.update(message);
    let result = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

/// Encrypt plaintext using NIP-44 v2.
///
/// Returns a base64-encoded payload: `version(1) || nonce(32) || ciphertext || mac(32)`.
///
/// # Errors
///
/// Returns [`Error::Crypto`] if encryption fails.
pub fn nip44_encrypt(conversation_key: &[u8; 32], plaintext: &[u8]) -> Result<String> {
    // Generate random 32-byte nonce.
    let mut nonce = [0u8; 32];
    AeadOsRng.fill_bytes(&mut nonce);

    nip44_encrypt_with_nonce(conversation_key, plaintext, &nonce)
}

/// Encrypt with explicit nonce (for testing determinism).
///
/// # Errors
///
/// Returns [`Error::Crypto`] if encryption fails.
pub fn nip44_encrypt_with_nonce(
    conversation_key: &[u8; 32],
    plaintext: &[u8],
    nonce: &[u8; 32],
) -> Result<String> {
    let (chacha_key, chacha_nonce, hmac_key) = get_message_keys(conversation_key, nonce)?;

    // Pad plaintext.
    let padded = pad(plaintext)?;

    // Encrypt with ChaCha20 (counter starts at 0).
    let mut ciphertext = padded;
    let mut cipher = ChaCha20::new(chacha_key.as_ref().into(), chacha_nonce.as_ref().into());
    cipher.apply_keystream(&mut ciphertext);

    // Compute MAC: HMAC-SHA256(hmac_key, nonce || ciphertext).
    let mac = hmac_aad(&hmac_key, &ciphertext, nonce);

    // Encode: version(1) || nonce(32) || ciphertext || mac(32)
    let mut payload = Vec::with_capacity(1 + 32 + ciphertext.len() + 32);
    payload.push(NIP44_VERSION);
    payload.extend_from_slice(nonce);
    payload.extend_from_slice(&ciphertext);
    payload.extend_from_slice(&mac);

    Ok(BASE64.encode(&payload))
}

/// Decrypt a NIP-44 v2 payload.
///
/// `payload` is the base64-encoded string from the event content.
///
/// # Errors
///
/// Returns [`Error::Crypto`] if the MAC doesn't match or decryption fails.
pub fn nip44_decrypt(conversation_key: &[u8; 32], payload: &str) -> Result<Vec<u8>> {
    // Check for future non-base64 encoding flag.
    if payload.starts_with('#') {
        return Err(Error::Crypto("unsupported NIP-44 encoding version".into()));
    }

    // Minimum base64 length: 132 chars per spec.
    if payload.len() < 132 {
        return Err(Error::Crypto("NIP-44 payload too short".into()));
    }

    // Base64 decode.
    let data = BASE64
        .decode(payload)
        .map_err(|e| Error::Crypto(format!("base64 decode failed: {e}")))?;

    // Minimum decoded length: 99 bytes per spec.
    if data.len() < 99 {
        return Err(Error::Crypto("NIP-44 decoded data too short".into()));
    }

    // Parse version.
    let version = data[0];
    if version != NIP44_VERSION {
        return Err(Error::Crypto(format!(
            "unsupported NIP-44 version: {version}"
        )));
    }

    // Parse nonce, ciphertext, mac.
    let nonce: [u8; 32] = data[1..33]
        .try_into()
        .map_err(|_| Error::Crypto("nonce parse failed".into()))?;
    let mac: [u8; 32] = data[data.len() - 32..]
        .try_into()
        .map_err(|_| Error::Crypto("mac parse failed".into()))?;
    let ciphertext = &data[33..data.len() - 32];

    // Derive message keys.
    let (chacha_key, chacha_nonce, hmac_key) = get_message_keys(conversation_key, &nonce)?;

    // Verify MAC (constant-time via hmac crate).
    let expected_mac = hmac_aad(&hmac_key, ciphertext, &nonce);
    if !constant_time_eq(&expected_mac, &mac) {
        return Err(Error::Crypto("NIP-44 MAC verification failed".into()));
    }

    // Decrypt with ChaCha20.
    let mut padded = ciphertext.to_vec();
    let mut cipher = ChaCha20::new(chacha_key.as_ref().into(), chacha_nonce.as_ref().into());
    cipher.apply_keystream(&mut padded);

    // Remove padding.
    unpad(&padded)
}

/// Constant-time byte array comparison.
fn constant_time_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    let mut acc: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        acc |= x ^ y;
    }
    acc == 0
}

// ─── NIP-59: Gift Wrap ────────────────────────────────────────────────────────

/// Create a NIP-59 gift-wrapped event.
///
/// Gift-wrap hides the true sender, recipient, and event kind from relay
/// operators by triple-wrapping:
///
/// 1. **Rumor** — unsigned event with the actual content (kind, tags, content).
/// 2. **Seal** — the rumor JSON is NIP-44 encrypted to `recipient_pubkey` and
///    placed in a `kind: 13` event signed by the sender.
/// 3. **Gift-wrap** — the seal JSON is NIP-44 encrypted to `recipient_pubkey`
///    and placed in a `kind: 1059` event signed by a random ephemeral key.
///
/// # Errors
///
/// Returns [`Error::Crypto`] on any failure.
pub fn gift_wrap(
    sender_secret_key: &Zeroizing<Vec<u8>>,
    recipient_pubkey_hex: &str,
    inner_content: &str,
    inner_kind: u32,
    inner_tags: &[Vec<String>],
) -> Result<NostrEvent> {
    let now = chrono::Utc::now().timestamp();

    // 1. Build the rumor (unsigned inner event).
    let sender_sk = SecretKey::from_slice(sender_secret_key)
        .map_err(|e| Error::Crypto(format!("invalid sender key: {e}")))?;
    let sender_pubkey = secret_key_to_pubkey_hex(&sender_sk);

    let rumor_id = compute_event_id(&sender_pubkey, now, inner_kind, inner_tags, inner_content);

    let rumor = serde_json::json!({
        "id": rumor_id,
        "pubkey": sender_pubkey,
        "created_at": now,
        "kind": inner_kind,
        "tags": inner_tags,
        "content": inner_content,
        "sig": "",
    });
    let rumor_json = serde_json::to_string(&rumor)
        .map_err(|e| Error::Serialisation(format!("rumor serialisation: {e}")))?;

    // 2. Seal: encrypt rumor to recipient, sign with sender key.
    let conversation_key = get_conversation_key(sender_secret_key, recipient_pubkey_hex)?;
    let sealed_content = nip44_encrypt(&conversation_key, rumor_json.as_bytes())?;

    // Randomise seal timestamp (±2 days) to reduce metadata leakage.
    let seal_ts = now - random_offset();
    let seal_event = sign_event(
        sender_secret_key,
        &sealed_content,
        13, // kind: seal
        vec![],
        seal_ts,
    )?;
    let seal_json = serde_json::to_string(&seal_event)
        .map_err(|e| Error::Serialisation(format!("seal serialisation: {e}")))?;

    // 3. Gift-wrap: encrypt seal to recipient, sign with ephemeral random key.
    let mut ephemeral_key_bytes = Zeroizing::new([0u8; 32]);
    AeadOsRng.fill_bytes(ephemeral_key_bytes.as_mut_slice());

    // Ensure the ephemeral key is a valid scalar (non-zero, < order).
    let ephemeral_sk = loop {
        match SecretKey::from_slice(ephemeral_key_bytes.as_slice()) {
            Ok(sk) => break sk,
            Err(_) => {
                AeadOsRng.fill_bytes(ephemeral_key_bytes.as_mut_slice());
            }
        }
    };
    let ephemeral_secret = Zeroizing::new(ephemeral_key_bytes.to_vec());

    let wrap_conversation_key =
        get_conversation_key(ephemeral_key_bytes.as_slice(), recipient_pubkey_hex)?;
    let wrapped_content = nip44_encrypt(&wrap_conversation_key, seal_json.as_bytes())?;

    // Randomise gift-wrap timestamp.
    let wrap_ts = now - random_offset();
    let _ = ephemeral_sk; // keep alive for zeroing
    let wrap_event = sign_event(
        &ephemeral_secret,
        &wrapped_content,
        1059, // kind: gift-wrap
        vec![vec!["p".to_string(), recipient_pubkey_hex.to_string()]],
        wrap_ts,
    )?;

    Ok(wrap_event)
}

/// Unwrap a NIP-59 gift-wrapped event.
///
/// Returns the inner rumor as a `NostrEvent` (unsigned — `sig` will be empty).
///
/// # Errors
///
/// Returns [`Error::Crypto`] if decryption or verification fails.
pub fn unwrap_gift_wrap(
    recipient_secret_key: &Zeroizing<Vec<u8>>,
    gift_wrap_event: &NostrEvent,
) -> Result<NostrEvent> {
    // 1. Decrypt the gift-wrap content using the ephemeral sender's pubkey.
    let wrap_conversation_key =
        get_conversation_key(recipient_secret_key, &gift_wrap_event.pubkey)?;
    let seal_json = nip44_decrypt(&wrap_conversation_key, &gift_wrap_event.content)?;
    let seal_json_str = String::from_utf8(seal_json)
        .map_err(|e| Error::Crypto(format!("seal is not valid UTF-8: {e}")))?;

    // 2. Parse the seal event.
    let seal_event: NostrEvent = serde_json::from_str(&seal_json_str)
        .map_err(|e| Error::Serialisation(format!("seal parse failed: {e}")))?;

    // Verify seal signature.
    verify_event(&seal_event)?;

    // 3. Decrypt the seal content using the sender's pubkey from the seal.
    let seal_conversation_key = get_conversation_key(recipient_secret_key, &seal_event.pubkey)?;
    let rumor_json = nip44_decrypt(&seal_conversation_key, &seal_event.content)?;
    let rumor_json_str = String::from_utf8(rumor_json)
        .map_err(|e| Error::Crypto(format!("rumor is not valid UTF-8: {e}")))?;

    // 4. Parse the rumor (unsigned event).
    let rumor: NostrEvent = serde_json::from_str(&rumor_json_str)
        .map_err(|e| Error::Serialisation(format!("rumor parse failed: {e}")))?;

    Ok(rumor)
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Convert a secret key to an x-only public key hex string (64 chars).
fn secret_key_to_pubkey_hex(sk: &SecretKey) -> String {
    let pk = sk.public_key();
    let point = pk.to_encoded_point(true); // compressed: [prefix][x-coord]
    let bytes = point.as_bytes();
    // bytes[0] is 0x02 or 0x03; bytes[1..33] is x-coordinate.
    hex::encode(&bytes[1..33])
}

/// Generate a random offset between 0 and 172800 seconds (2 days) for
/// timestamp randomisation in gift-wrap.
fn random_offset() -> i64 {
    let mut buf = [0u8; 4];
    AeadOsRng.fill_bytes(&mut buf);
    let val = u32::from_le_bytes(buf);
    i64::from(val % 172_800)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::similar_names)]
mod tests {
    use super::*;

    // ── NIP-44 padding tests ──────────────────────────────────────────────────

    #[test]
    fn calc_padded_len_small_values() {
        assert_eq!(calc_padded_len(1), 32);
        assert_eq!(calc_padded_len(31), 32);
        assert_eq!(calc_padded_len(32), 32);
        assert_eq!(calc_padded_len(33), 64);
        assert_eq!(calc_padded_len(64), 64);
        assert_eq!(calc_padded_len(65), 96);
    }

    #[test]
    fn calc_padded_len_medium_values() {
        assert_eq!(calc_padded_len(256), 256);
        assert_eq!(calc_padded_len(257), 288);
        assert_eq!(calc_padded_len(289), 320);
        assert_eq!(calc_padded_len(320), 320);
    }

    #[test]
    fn pad_unpad_roundtrip_short() {
        let plaintext = b"hello world";
        let padded = pad(plaintext).unwrap();
        // 2 byte prefix + 32 bytes padded = 34 total
        assert_eq!(padded.len(), 2 + 32);
        let recovered = unpad(&padded).unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn pad_unpad_roundtrip_exact_32() {
        let plaintext = vec![0x41u8; 32];
        let padded = pad(&plaintext).unwrap();
        assert_eq!(padded.len(), 2 + 32);
        let recovered = unpad(&padded).unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn pad_unpad_roundtrip_33_bytes() {
        let plaintext = vec![0x42u8; 33];
        let padded = pad(&plaintext).unwrap();
        // Padded len for 33 = 64; prefix = 2; total = 66
        assert_eq!(padded.len(), 2 + 64);
        let recovered = unpad(&padded).unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn pad_empty_returns_error() {
        let result = pad(b"");
        assert!(result.is_err());
    }

    // ── NIP-44 conversation key ──────────────────────────────────────────────

    #[test]
    fn conversation_key_symmetric() {
        // Key A and key B should produce the same conversation key regardless
        // of role (sender/recipient).
        let sk_a = hex::decode("0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap();
        let sk_b = hex::decode("0000000000000000000000000000000000000000000000000000000000000002")
            .unwrap();

        // Derive pubkeys.
        let pub_a = secret_key_to_pubkey_hex(&SecretKey::from_slice(&sk_a).unwrap());
        let pub_b = secret_key_to_pubkey_hex(&SecretKey::from_slice(&sk_b).unwrap());

        let ck_ab = get_conversation_key(&sk_a, &pub_b).unwrap();
        let ck_ba = get_conversation_key(&sk_b, &pub_a).unwrap();

        assert_eq!(*ck_ab, *ck_ba, "conversation key must be symmetric");
    }

    #[test]
    fn conversation_key_known_vector() {
        // From NIP-44 test vectors:
        // sec1 = "0000...0001", sec2 = "0000...0002"
        // conversation_key = "c41c775356fd92eadc63ff5a0dc1da211b268cbea22316767095b2871ea1412d"
        let sk_a = hex::decode("0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap();
        let sk_b = SecretKey::from_slice(
            &hex::decode("0000000000000000000000000000000000000000000000000000000000000002")
                .unwrap(),
        )
        .unwrap();
        let pub_b = secret_key_to_pubkey_hex(&sk_b);

        let ck = get_conversation_key(&sk_a, &pub_b).unwrap();
        let expected =
            hex::decode("c41c775356fd92eadc63ff5a0dc1da211b268cbea22316767095b2871ea1412d")
                .unwrap();
        assert_eq!(ck.as_slice(), expected.as_slice());
    }

    // ── NIP-44 encrypt/decrypt roundtrip ─────────────────────────────────────

    #[test]
    fn nip44_encrypt_decrypt_roundtrip() {
        let sk_a = hex::decode("0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap();
        let sk_b_raw =
            hex::decode("0000000000000000000000000000000000000000000000000000000000000002")
                .unwrap();
        let sk_b = SecretKey::from_slice(&sk_b_raw).unwrap();
        let pub_b = secret_key_to_pubkey_hex(&sk_b);

        let ck = get_conversation_key(&sk_a, &pub_b).unwrap();
        let plaintext = b"hello nostr";

        let encrypted = nip44_encrypt(&ck, plaintext).unwrap();
        let decrypted = nip44_decrypt(&ck, &encrypted).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn nip44_encrypt_decrypt_long_message() {
        let sk_a = hex::decode("7f7ff03d123792d6ac594bfa67bf6d0c0ab55b6b1fdb6249303fe861f1ccba9a")
            .unwrap();
        let sk_b_raw =
            hex::decode("c02e0ce7aa56df52c8f2b7e2e2fa3f5f3a5d8e3f1a7b5c4d6e2f8a9b0c1d2e3f")
                .unwrap();
        let sk_b = SecretKey::from_slice(&sk_b_raw).unwrap();
        let pub_b = secret_key_to_pubkey_hex(&sk_b);

        let ck = get_conversation_key(&sk_a, &pub_b).unwrap();
        let plaintext = vec![0x61u8; 1000]; // 1000 'a' bytes

        let encrypted = nip44_encrypt(&ck, &plaintext).unwrap();
        let decrypted = nip44_decrypt(&ck, &encrypted).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn nip44_decrypt_wrong_key_fails() {
        let secret_alice =
            hex::decode("0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();
        let secret_bob =
            hex::decode("0000000000000000000000000000000000000000000000000000000000000002")
                .unwrap();
        let secret_carol =
            hex::decode("0000000000000000000000000000000000000000000000000000000000000003")
                .unwrap();
        let bob = SecretKey::from_slice(&secret_bob).unwrap();
        let carol = SecretKey::from_slice(&secret_carol).unwrap();
        let pubkey_bob = secret_key_to_pubkey_hex(&bob);
        let pubkey_carol = secret_key_to_pubkey_hex(&carol);

        let conv_alice_bob = get_conversation_key(&secret_alice, &pubkey_bob).unwrap();
        let conv_alice_carol = get_conversation_key(&secret_alice, &pubkey_carol).unwrap();

        let encrypted = nip44_encrypt(&conv_alice_bob, b"secret").unwrap();

        // Trying to decrypt with wrong conversation key should fail.
        let result = nip44_decrypt(&conv_alice_carol, &encrypted);
        assert!(result.is_err(), "decryption with wrong key must fail");
    }

    #[test]
    fn nip44_known_vector_encrypt_decrypt() {
        // NIP-44 test vector: sec1=0x01, sec2=0x02, nonce=0x01 repeated,
        // plaintext="a", expected payload starts with "AgAAAA..."
        let sk_a = hex::decode("0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap();
        let sk_b_raw =
            hex::decode("0000000000000000000000000000000000000000000000000000000000000002")
                .unwrap();
        let sk_b = SecretKey::from_slice(&sk_b_raw).unwrap();
        let pub_b = secret_key_to_pubkey_hex(&sk_b);

        let ck = get_conversation_key(&sk_a, &pub_b).unwrap();

        let nonce = [0u8; 32];
        let mut nonce_with_one = nonce;
        nonce_with_one[31] = 1;

        let payload = nip44_encrypt_with_nonce(&ck, b"a", &nonce_with_one).unwrap();

        // Verify we can decrypt it back.
        let decrypted = nip44_decrypt(&ck, &payload).unwrap();
        assert_eq!(decrypted, b"a");

        // Verify the payload matches the expected from the NIP-44 spec.
        let expected = "AgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABee0G5VSK0/9YypIObAtDKfYEAjD35uVkHyB0F4DwrcNaCXlCWZKaArsGrY6M9wnuTMxWfp1RTN9Xga8no+kF5Vsb";
        assert_eq!(payload, expected, "must match NIP-44 test vector");
    }

    // ── NIP-01 event signing ──────────────────────────────────────────────────

    #[test]
    fn sign_event_produces_valid_event() {
        let sk = Zeroizing::new(
            hex::decode("0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap(),
        );

        let event = sign_event(&sk, "hello", 1, vec![], 1_700_000_000).unwrap();

        assert_eq!(event.pubkey.len(), 64);
        assert_eq!(event.id.len(), 64);
        assert_eq!(event.sig.len(), 128);
        assert_eq!(event.kind, 1);
        assert_eq!(event.content, "hello");
    }

    #[test]
    fn sign_and_verify_event() {
        let sk = Zeroizing::new(
            hex::decode("7f7ff03d123792d6ac594bfa67bf6d0c0ab55b6b1fdb6249303fe861f1ccba9a")
                .unwrap(),
        );

        let event = sign_event(
            &sk,
            "test content",
            4242,
            vec![vec!["p".to_string(), "abc".to_string()]],
            1_700_000_000,
        )
        .unwrap();

        // Verify should pass.
        verify_event(&event).unwrap();
    }

    #[test]
    fn verify_event_tampered_content_fails() {
        let sk = Zeroizing::new(
            hex::decode("0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap(),
        );

        let mut event = sign_event(&sk, "original", 1, vec![], 1_700_000_000).unwrap();
        event.content = "tampered".to_string();

        let result = verify_event(&event);
        assert!(result.is_err(), "tampered content must fail verification");
    }

    // ── NIP-59 gift-wrap ──────────────────────────────────────────────────────

    #[test]
    fn gift_wrap_and_unwrap_roundtrip() {
        let sender_sk = Zeroizing::new(
            hex::decode("0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap(),
        );
        let recipient_sk = Zeroizing::new(
            hex::decode("0000000000000000000000000000000000000000000000000000000000000002")
                .unwrap(),
        );
        let recipient_pk = secret_key_to_pubkey_hex(&SecretKey::from_slice(&recipient_sk).unwrap());

        let wrapped = gift_wrap(
            &sender_sk,
            &recipient_pk,
            "secret vault data",
            10050, // custom vault sync kind
            &[],
        )
        .unwrap();

        // Gift-wrap should be kind 1059.
        assert_eq!(wrapped.kind, 1059);
        // The pubkey should NOT be the sender's.
        let sender_pk = secret_key_to_pubkey_hex(&SecretKey::from_slice(&sender_sk).unwrap());
        assert_ne!(
            wrapped.pubkey, sender_pk,
            "gift-wrap must use ephemeral key"
        );

        // Unwrap.
        let rumor = unwrap_gift_wrap(&recipient_sk, &wrapped).unwrap();
        assert_eq!(rumor.content, "secret vault data");
        assert_eq!(rumor.kind, 10050);
        assert_eq!(rumor.pubkey, sender_pk);
    }

    #[test]
    fn gift_wrap_unwrap_by_wrong_recipient_fails() {
        let sender_sk = Zeroizing::new(
            hex::decode("0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap(),
        );
        let recipient_sk = Zeroizing::new(
            hex::decode("0000000000000000000000000000000000000000000000000000000000000002")
                .unwrap(),
        );
        let wrong_sk = Zeroizing::new(
            hex::decode("0000000000000000000000000000000000000000000000000000000000000003")
                .unwrap(),
        );
        let recipient_pk = secret_key_to_pubkey_hex(&SecretKey::from_slice(&recipient_sk).unwrap());

        let wrapped = gift_wrap(&sender_sk, &recipient_pk, "secret", 1, &[]).unwrap();

        // Wrong recipient should fail to unwrap.
        let result = unwrap_gift_wrap(&wrong_sk, &wrapped);
        assert!(result.is_err(), "wrong recipient must fail unwrap");
    }

    // ── Edge-case tests (security review) ─────────────────────────────────────

    #[test]
    fn nip44_tampered_mac_rejected() {
        let sk_a = hex::decode("0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap();
        let sk_b_raw =
            hex::decode("0000000000000000000000000000000000000000000000000000000000000002")
                .unwrap();
        let sk_b = SecretKey::from_slice(&sk_b_raw).unwrap();
        let pub_b = secret_key_to_pubkey_hex(&sk_b);

        let ck = get_conversation_key(&sk_a, &pub_b).unwrap();
        let encrypted = nip44_encrypt(&ck, b"sensitive data").unwrap();

        // Decode, tamper with the MAC, re-encode.
        let mut data = BASE64.decode(&encrypted).unwrap();
        let mac_start = data.len() - 32;
        data[mac_start] ^= 0xFF; // flip a bit in the MAC
        let tampered = BASE64.encode(&data);

        let result = nip44_decrypt(&ck, &tampered);
        assert!(result.is_err(), "tampered MAC must be rejected");
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("MAC verification failed"),
            "error should mention MAC failure, got: {err_msg}"
        );
    }

    #[test]
    fn nip44_tampered_ciphertext_rejected() {
        let sk_a = hex::decode("0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap();
        let sk_b_raw =
            hex::decode("0000000000000000000000000000000000000000000000000000000000000002")
                .unwrap();
        let sk_b = SecretKey::from_slice(&sk_b_raw).unwrap();
        let pub_b = secret_key_to_pubkey_hex(&sk_b);

        let ck = get_conversation_key(&sk_a, &pub_b).unwrap();
        let encrypted = nip44_encrypt(&ck, b"test payload").unwrap();

        // Decode, tamper with ciphertext (not MAC), re-encode.
        let mut data = BASE64.decode(&encrypted).unwrap();
        // Flip a byte in the ciphertext region (after version + nonce, before MAC).
        let ct_start = 1 + 32; // version(1) + nonce(32)
        data[ct_start + 5] ^= 0xFF;
        let tampered = BASE64.encode(&data);

        let result = nip44_decrypt(&ck, &tampered);
        assert!(
            result.is_err(),
            "tampered ciphertext must be rejected by MAC check"
        );
    }

    #[test]
    fn nip44_payload_too_short_rejected() {
        let ck = [0u8; 32];
        // Very short payload (below 132 chars minimum).
        let result = nip44_decrypt(&ck, "AAAA");
        assert!(result.is_err());
    }

    #[test]
    fn nip44_unsupported_version_rejected() {
        let ck = [0u8; 32];
        // Construct a payload with version byte = 3 (unsupported).
        let mut data = vec![3u8]; // wrong version
        data.extend_from_slice(&[0u8; 32]); // nonce
        data.extend_from_slice(&[0u8; 64]); // fake ciphertext (enough to pass length check)
        data.extend_from_slice(&[0u8; 32]); // fake mac
        let payload = BASE64.encode(&data);

        let result = nip44_decrypt(&ck, &payload);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("unsupported NIP-44 version"));
    }

    #[test]
    fn nip44_padding_boundary_32_to_33() {
        // Verify that messages at the 32→33 byte boundary are handled correctly.
        let sk_a = hex::decode("0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap();
        let sk_b_raw =
            hex::decode("0000000000000000000000000000000000000000000000000000000000000002")
                .unwrap();
        let sk_b = SecretKey::from_slice(&sk_b_raw).unwrap();
        let pub_b = secret_key_to_pubkey_hex(&sk_b);

        let ck = get_conversation_key(&sk_a, &pub_b).unwrap();

        // Test exact boundary: 32 bytes and 33 bytes.
        let msg_32 = vec![0x61u8; 32];
        let msg_33 = vec![0x61u8; 33];

        let enc_32 = nip44_encrypt(&ck, &msg_32).unwrap();
        let enc_33 = nip44_encrypt(&ck, &msg_33).unwrap();

        let dec_32 = nip44_decrypt(&ck, &enc_32).unwrap();
        let dec_33 = nip44_decrypt(&ck, &enc_33).unwrap();

        assert_eq!(dec_32, msg_32);
        assert_eq!(dec_33, msg_33);

        // 32-byte message pads to 32; 33-byte message pads to 64.
        // This means the ciphertext lengths should differ.
        let data_32 = BASE64.decode(&enc_32).unwrap();
        let data_33 = BASE64.decode(&enc_33).unwrap();
        assert!(
            data_33.len() > data_32.len(),
            "33-byte message should produce larger ciphertext than 32-byte"
        );
    }
}

// ─── Property-based tests ────────────────────────────────────────────────────

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn nip44_encrypt_decrypt_roundtrip_arbitrary(
            message in proptest::collection::vec(any::<u8>(), 1..2048)
        ) {
            // Use fixed keys for deterministic conversation key derivation.
            let sk_a = [0x01u8; 32];
            // Construct a valid secret key for party B.
            let mut sk_b_bytes = [0u8; 32];
            sk_b_bytes[31] = 0x02;
            let sk_b = k256::SecretKey::from_slice(&sk_b_bytes).unwrap();
            let pub_b = secret_key_to_pubkey_hex(&sk_b);

            let ck = get_conversation_key(&sk_a, &pub_b).unwrap();
            let encrypted = nip44_encrypt(&ck, &message).unwrap();
            let decrypted = nip44_decrypt(&ck, &encrypted).unwrap();
            prop_assert_eq!(decrypted, message);
        }

        #[test]
        fn nip44_decrypt_never_panics_on_arbitrary_input(
            data in proptest::collection::vec(any::<u8>(), 0..512)
        ) {
            let conversation_key = [0x42u8; 32];
            if let Ok(payload) = std::str::from_utf8(&data) {
                // Must not panic — errors are acceptable.
                let _ = nip44_decrypt(&conversation_key, payload);
            }
        }
    }
}
