//! Device pairing codec: encode/decode invite and join-request codes.
//!
//! Pairing codes are exchanged between devices to establish trust. They are
//! human-readable strings starting with `zvault:` followed by a base64url-encoded
//! JSON payload.
//!
//! ## Protocol
//!
//! 1. **Invite flow:** Device A generates an invite code → B scans/pastes →
//!    B creates an invite-response → A admits B.
//! 2. **Join-request flow:** Device B generates a join-request → A scans/pastes →
//!    A creates a join-response and admits B.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Error, Result};

/// Prefix for all pairing codes.
pub const PAIRING_PREFIX: &str = "zvault:";

/// Maximum allowed length for a pairing code (including prefix).
pub const MAX_CODE_LENGTH: usize = 500;

// ─── PairingType ─────────────────────────────────────────────────────────────

/// The type of pairing message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingType {
    /// An invitation from an existing device to a new device.
    Invite,
    /// A join request from a new device to an existing vault.
    JoinRequest,
    /// Response to an invite (from the joining device).
    InviteResponse,
    /// Response to a join request (from the admin device).
    JoinResponse,
}

// ─── PairingPayload ──────────────────────────────────────────────────────────

/// The JSON payload embedded in a pairing code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingPayload {
    /// Protocol version (must be 1).
    pub v: u8,
    /// Message type.
    pub t: PairingType,
    /// secp256k1 public key of the sender (hex, 64 chars).
    pub p: String,
    /// Human-readable device label of the sender.
    pub l: String,
    /// Vault ID (present in Invite and JoinResponse; absent in JoinRequest and InviteResponse).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vid: Option<Uuid>,
    /// Unix timestamp (seconds) when the code was generated.
    pub ts: i64,
}

// ─── Builder functions ───────────────────────────────────────────────────────

/// Create an invite payload from an existing vault device.
///
/// The invite contains the sender's public key, label, and vault ID so the
/// recipient knows which vault they are being invited to.
///
/// # Errors
///
/// Returns [`Error::InvalidPairingCode`] if the public key or label is invalid.
pub fn create_invite(pubkey_hex: &str, label: &str, vault_id: Uuid) -> Result<PairingPayload> {
    validate_pubkey(pubkey_hex)?;
    validate_label(label)?;
    Ok(PairingPayload {
        v: 1,
        t: PairingType::Invite,
        p: pubkey_hex.to_lowercase(),
        l: label.trim().to_string(),
        vid: Some(vault_id),
        ts: Utc::now().timestamp(),
    })
}

/// Create a join-request payload from a device that wants to join a vault.
///
/// The join request contains the sender's public key and label. No vault ID
/// is included because the sender does not yet know it.
///
/// # Errors
///
/// Returns [`Error::InvalidPairingCode`] if the public key or label is invalid.
pub fn create_join_request(pubkey_hex: &str, label: &str) -> Result<PairingPayload> {
    validate_pubkey(pubkey_hex)?;
    validate_label(label)?;
    Ok(PairingPayload {
        v: 1,
        t: PairingType::JoinRequest,
        p: pubkey_hex.to_lowercase(),
        l: label.trim().to_string(),
        vid: None,
        ts: Utc::now().timestamp(),
    })
}

/// Create an invite-response payload (from the joining device back to the inviter).
///
/// Contains the joining device's public key and label.
///
/// # Errors
///
/// Returns [`Error::InvalidPairingCode`] if the public key or label is invalid.
pub fn create_invite_response(pubkey_hex: &str, label: &str) -> Result<PairingPayload> {
    validate_pubkey(pubkey_hex)?;
    validate_label(label)?;
    Ok(PairingPayload {
        v: 1,
        t: PairingType::InviteResponse,
        p: pubkey_hex.to_lowercase(),
        l: label.trim().to_string(),
        vid: None,
        ts: Utc::now().timestamp(),
    })
}

/// Create a join-response payload (from the admin accepting a join request).
///
/// Contains the admin's public key, label, and vault ID.
///
/// # Errors
///
/// Returns [`Error::InvalidPairingCode`] if the public key or label is invalid.
pub fn create_join_response(
    pubkey_hex: &str,
    label: &str,
    vault_id: Uuid,
) -> Result<PairingPayload> {
    validate_pubkey(pubkey_hex)?;
    validate_label(label)?;
    Ok(PairingPayload {
        v: 1,
        t: PairingType::JoinResponse,
        p: pubkey_hex.to_lowercase(),
        l: label.trim().to_string(),
        vid: Some(vault_id),
        ts: Utc::now().timestamp(),
    })
}

// ─── Encode / Decode ─────────────────────────────────────────────────────────

/// Encode a pairing payload into a `zvault:` prefixed string.
///
/// The format is: `zvault:` + base64url(JSON(payload)).
///
/// # Errors
///
/// Returns [`Error::InvalidPairingCode`] if the resulting code exceeds
/// [`MAX_CODE_LENGTH`] characters.
pub fn encode_pairing_code(payload: &PairingPayload) -> Result<String> {
    let json = serde_json::to_vec(payload)
        .map_err(|e| Error::InvalidPairingCode(format!("serialisation failed: {e}")))?;
    let b64 = URL_SAFE_NO_PAD.encode(&json);
    let code = format!("{PAIRING_PREFIX}{b64}");
    if code.len() > MAX_CODE_LENGTH {
        return Err(Error::InvalidPairingCode(format!(
            "code exceeds maximum length of {MAX_CODE_LENGTH} chars (got {})",
            code.len()
        )));
    }
    Ok(code)
}

/// Decode a `zvault:` prefixed pairing code into a [`PairingPayload`].
///
/// # Errors
///
/// Returns [`Error::InvalidPairingCode`] if:
/// - The prefix is missing or wrong
/// - Base64 decoding fails
/// - JSON parsing fails
/// - Validation fails (bad version, bad pubkey, bad label, bad vid)
pub fn decode_pairing_code(code: &str) -> Result<PairingPayload> {
    let code = code.trim();
    let b64 = code
        .strip_prefix(PAIRING_PREFIX)
        .ok_or_else(|| Error::InvalidPairingCode("missing 'zvault:' prefix".into()))?;

    if b64.is_empty() {
        return Err(Error::InvalidPairingCode("empty payload".into()));
    }

    let json_bytes = URL_SAFE_NO_PAD
        .decode(b64)
        .map_err(|e| Error::InvalidPairingCode(format!("base64 decode failed: {e}")))?;

    let payload: PairingPayload = serde_json::from_slice(&json_bytes)
        .map_err(|e| Error::InvalidPairingCode(format!("JSON parse failed: {e}")))?;

    validate_payload(&payload)?;
    Ok(payload)
}

// ─── Validation ──────────────────────────────────────────────────────────────

/// Validate a decoded payload.
fn validate_payload(payload: &PairingPayload) -> Result<()> {
    if payload.v != 1 {
        return Err(Error::InvalidPairingCode(format!(
            "unsupported version: {}",
            payload.v
        )));
    }
    validate_pubkey(&payload.p)?;
    validate_label(&payload.l)?;
    if payload.ts <= 0 {
        return Err(Error::InvalidPairingCode(
            "timestamp must be positive".into(),
        ));
    }
    // vid must be valid UUID if present (serde handles this via the Uuid type,
    // but check it's not the nil UUID).
    if let Some(vid) = payload.vid {
        if vid.is_nil() {
            return Err(Error::InvalidPairingCode("vault ID must not be nil".into()));
        }
    }
    Ok(())
}

/// Validate a hex-encoded public key (must be 64 hex chars).
fn validate_pubkey(pubkey: &str) -> Result<()> {
    if pubkey.len() != 64 {
        return Err(Error::InvalidPairingCode(format!(
            "public key must be 64 hex chars, got {}",
            pubkey.len()
        )));
    }
    if !pubkey.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::InvalidPairingCode(
            "public key contains non-hex characters".into(),
        ));
    }
    Ok(())
}

/// Validate a device label (1-64 chars after trimming).
fn validate_label(label: &str) -> Result<()> {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidPairingCode("label must not be empty".into()));
    }
    if trimmed.len() > 64 {
        return Err(Error::InvalidPairingCode(format!(
            "label must be 1-64 chars, got {}",
            trimmed.len()
        )));
    }
    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PUBKEY: &str = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";

    // ── Round-trip tests ──────────────────────────────────────────────────────

    #[test]
    fn roundtrip_invite() {
        let vault_id = Uuid::new_v4();
        let payload = create_invite(TEST_PUBKEY, "Alice's MacBook", vault_id).unwrap();
        let code = encode_pairing_code(&payload).unwrap();
        assert!(code.starts_with(PAIRING_PREFIX));
        assert!(code.len() <= MAX_CODE_LENGTH);

        let decoded = decode_pairing_code(&code).unwrap();
        assert_eq!(decoded.v, 1);
        assert_eq!(decoded.t, PairingType::Invite);
        assert_eq!(decoded.p, TEST_PUBKEY);
        assert_eq!(decoded.l, "Alice's MacBook");
        assert_eq!(decoded.vid, Some(vault_id));
        assert!(decoded.ts > 0);
    }

    #[test]
    fn roundtrip_join_request() {
        let payload = create_join_request(TEST_PUBKEY, "Bob's Phone").unwrap();
        let code = encode_pairing_code(&payload).unwrap();
        assert!(code.starts_with(PAIRING_PREFIX));
        assert!(code.len() <= MAX_CODE_LENGTH);

        let decoded = decode_pairing_code(&code).unwrap();
        assert_eq!(decoded.t, PairingType::JoinRequest);
        assert_eq!(decoded.p, TEST_PUBKEY);
        assert_eq!(decoded.l, "Bob's Phone");
        assert!(decoded.vid.is_none());
    }

    #[test]
    fn roundtrip_invite_response() {
        let payload = create_invite_response(TEST_PUBKEY, "New Device").unwrap();
        let code = encode_pairing_code(&payload).unwrap();
        let decoded = decode_pairing_code(&code).unwrap();
        assert_eq!(decoded.t, PairingType::InviteResponse);
        assert!(decoded.vid.is_none());
    }

    #[test]
    fn roundtrip_join_response() {
        let vault_id = Uuid::new_v4();
        let payload = create_join_response(TEST_PUBKEY, "Admin", vault_id).unwrap();
        let code = encode_pairing_code(&payload).unwrap();
        let decoded = decode_pairing_code(&code).unwrap();
        assert_eq!(decoded.t, PairingType::JoinResponse);
        assert_eq!(decoded.vid, Some(vault_id));
    }

    // ── Size check ───────────────────────────────────────────────────────────

    #[test]
    fn code_under_500_chars() {
        let vault_id = Uuid::new_v4();
        // Use maximum length label (64 chars)
        let long_label = "A".repeat(64);
        let payload = create_invite(TEST_PUBKEY, &long_label, vault_id).unwrap();
        let code = encode_pairing_code(&payload).unwrap();
        assert!(
            code.len() <= MAX_CODE_LENGTH,
            "code length {} exceeds {}",
            code.len(),
            MAX_CODE_LENGTH
        );
    }

    // ── Rejection tests ──────────────────────────────────────────────────────

    #[test]
    fn reject_missing_prefix() {
        let err = decode_pairing_code("not-a-zvault-code").unwrap_err();
        assert!(matches!(err, Error::InvalidPairingCode(_)));
    }

    #[test]
    fn reject_empty_payload() {
        let err = decode_pairing_code("zvault:").unwrap_err();
        assert!(matches!(err, Error::InvalidPairingCode(_)));
    }

    #[test]
    fn reject_bad_base64() {
        let err = decode_pairing_code("zvault:!!!invalid-base64!!!").unwrap_err();
        assert!(matches!(err, Error::InvalidPairingCode(_)));
    }

    #[test]
    fn reject_bad_json() {
        let b64 = URL_SAFE_NO_PAD.encode(b"not json at all");
        let code = format!("zvault:{b64}");
        let err = decode_pairing_code(&code).unwrap_err();
        assert!(matches!(err, Error::InvalidPairingCode(_)));
    }

    #[test]
    fn reject_bad_version() {
        let payload = PairingPayload {
            v: 99,
            t: PairingType::Invite,
            p: TEST_PUBKEY.to_string(),
            l: "Test".to_string(),
            vid: Some(Uuid::new_v4()),
            ts: Utc::now().timestamp(),
        };
        let json = serde_json::to_vec(&payload).unwrap();
        let b64 = URL_SAFE_NO_PAD.encode(&json);
        let code = format!("zvault:{b64}");
        let err = decode_pairing_code(&code).unwrap_err();
        assert!(matches!(err, Error::InvalidPairingCode(_)));
    }

    #[test]
    fn reject_bad_pubkey_short() {
        let err = create_invite("abcd", "Test", Uuid::new_v4()).unwrap_err();
        assert!(matches!(err, Error::InvalidPairingCode(_)));
    }

    #[test]
    fn reject_bad_pubkey_non_hex() {
        let bad_key = "g".repeat(64); // 'g' is not valid hex
        let err = create_invite(&bad_key, "Test", Uuid::new_v4()).unwrap_err();
        assert!(matches!(err, Error::InvalidPairingCode(_)));
    }

    #[test]
    fn reject_empty_label() {
        let err = create_invite(TEST_PUBKEY, "", Uuid::new_v4()).unwrap_err();
        assert!(matches!(err, Error::InvalidPairingCode(_)));
    }

    #[test]
    fn reject_whitespace_only_label() {
        let err = create_invite(TEST_PUBKEY, "   ", Uuid::new_v4()).unwrap_err();
        assert!(matches!(err, Error::InvalidPairingCode(_)));
    }

    #[test]
    fn reject_label_too_long() {
        let long_label = "A".repeat(65);
        let err = create_invite(TEST_PUBKEY, &long_label, Uuid::new_v4()).unwrap_err();
        assert!(matches!(err, Error::InvalidPairingCode(_)));
    }

    #[test]
    fn reject_negative_timestamp() {
        let payload = PairingPayload {
            v: 1,
            t: PairingType::Invite,
            p: TEST_PUBKEY.to_string(),
            l: "Test".to_string(),
            vid: Some(Uuid::new_v4()),
            ts: -100,
        };
        let json = serde_json::to_vec(&payload).unwrap();
        let b64 = URL_SAFE_NO_PAD.encode(&json);
        let code = format!("zvault:{b64}");
        let err = decode_pairing_code(&code).unwrap_err();
        assert!(matches!(err, Error::InvalidPairingCode(_)));
    }

    #[test]
    fn reject_nil_vault_id() {
        let payload = PairingPayload {
            v: 1,
            t: PairingType::Invite,
            p: TEST_PUBKEY.to_string(),
            l: "Test".to_string(),
            vid: Some(Uuid::nil()),
            ts: Utc::now().timestamp(),
        };
        let json = serde_json::to_vec(&payload).unwrap();
        let b64 = URL_SAFE_NO_PAD.encode(&json);
        let code = format!("zvault:{b64}");
        let err = decode_pairing_code(&code).unwrap_err();
        assert!(matches!(err, Error::InvalidPairingCode(_)));
    }

    // ── Builder validation ───────────────────────────────────────────────────

    #[test]
    fn builders_produce_valid_payloads() {
        let vid = Uuid::new_v4();
        let invite = create_invite(TEST_PUBKEY, "Admin", vid).unwrap();
        assert_eq!(invite.v, 1);
        assert_eq!(invite.t, PairingType::Invite);
        assert_eq!(invite.vid, Some(vid));

        let join_req = create_join_request(TEST_PUBKEY, "Requester").unwrap();
        assert_eq!(join_req.t, PairingType::JoinRequest);
        assert!(join_req.vid.is_none());

        let inv_resp = create_invite_response(TEST_PUBKEY, "Joiner").unwrap();
        assert_eq!(inv_resp.t, PairingType::InviteResponse);
        assert!(inv_resp.vid.is_none());

        let join_resp = create_join_response(TEST_PUBKEY, "Admin", vid).unwrap();
        assert_eq!(join_resp.t, PairingType::JoinResponse);
        assert_eq!(join_resp.vid, Some(vid));
    }

    #[test]
    fn label_is_trimmed() {
        let payload = create_invite(TEST_PUBKEY, "  spaces  ", Uuid::new_v4()).unwrap();
        assert_eq!(payload.l, "spaces");
    }

    #[test]
    fn pubkey_is_lowercased() {
        let upper_key = TEST_PUBKEY.to_uppercase();
        let payload = create_invite(&upper_key, "Test", Uuid::new_v4()).unwrap();
        assert_eq!(payload.p, TEST_PUBKEY);
    }
}

// ─── Property-based tests ────────────────────────────────────────────────────

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    /// Strategy for valid hex pubkey (64 hex chars).
    fn arb_pubkey() -> impl Strategy<Value = String> {
        "[0-9a-f]{64}"
    }

    /// Strategy for valid label (1-64 printable chars, starts with non-space).
    fn arb_label() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9_'-][a-zA-Z0-9 _'-]{0,63}"
    }

    proptest! {
        /// Encode/decode is a lossless round-trip for all valid payloads.
        #[test]
        fn encode_decode_roundtrip(pubkey in arb_pubkey(), label in arb_label()) {
            let vault_id = Uuid::new_v4();
            let payload = create_invite(&pubkey, &label, vault_id).unwrap();
            let code = encode_pairing_code(&payload).unwrap();
            let decoded = decode_pairing_code(&code).unwrap();
            prop_assert_eq!(decoded.p, pubkey);
            prop_assert_eq!(decoded.l, label.trim());
            prop_assert_eq!(decoded.vid, Some(vault_id));
        }

        /// decode_pairing_code never panics on arbitrary input.
        #[test]
        fn decode_never_panics(data in ".*") {
            let _ = decode_pairing_code(&data);
        }
    }
}
