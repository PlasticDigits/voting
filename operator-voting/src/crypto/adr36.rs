use base64::Engine;
use bech32::{ToBase32, Variant};
use k256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
use ripemd::Ripemd160;
use sha2::{Digest, Sha256};

use crate::error::{VotingError, VotingResult};

/// Keplr `signArbitrary` ADR-36 amino JSON (sorted keys, cosmjs-compatible).
pub fn adr36_sign_doc_json(signer: &str, message: &str) -> String {
    let data_b64 = base64::engine::general_purpose::STANDARD.encode(message.as_bytes());
    format!(
        r#"{{"account_number":"0","chain_id":"","fee":{{"amount":[],"gas":"0"}},"memo":"","msgs":[{{"type":"sign/MsgSignData","value":{{"data":"{data_b64}","signer":"{signer}"}}}}],"sequence":"0"}}"#
    )
}

pub fn cosmos_address_from_pubkey(compressed_pubkey: &[u8], hrp: &str) -> VotingResult<String> {
    let digest = Sha256::digest(compressed_pubkey);
    let rip = Ripemd160::digest(digest);
    bech32::encode(hrp, rip.to_base32(), Variant::Bech32)
        .map_err(|_| VotingError::BadRequest("bech32 encode failed".into()))
}

fn verify_prehash(pubkey: &[u8], hash: &[u8], sig: &[u8]) -> VotingResult<()> {
    use k256::ecdsa::signature::hazmat::PrehashVerifier;
    let vk = VerifyingKey::from_sec1_bytes(pubkey)
        .map_err(|_| VotingError::Unauthorized("invalid terra pubkey".into()))?;
    let sig = Signature::from_slice(sig)
        .map_err(|_| VotingError::Unauthorized("invalid terra signature".into()))?;
    vk.verify_prehash(hash, &sig)
        .map_err(|_| VotingError::Unauthorized("terra signature verification failed".into()))
}

/// Verify Keplr ADR-36 `signArbitrary` (signature + pubkey are base64).
pub fn verify_adr36(
    expected_address: &str,
    message: &str,
    signature_b64: &str,
    pubkey_b64: &str,
) -> VotingResult<()> {
    let hrp = expected_address
        .split_once('1')
        .map(|(p, _)| p)
        .ok_or_else(|| VotingError::BadRequest("invalid bech32 address".into()))?;
    let sig_bytes = base64::engine::general_purpose::STANDARD
        .decode(signature_b64)
        .map_err(|_| VotingError::BadRequest("invalid terra signature base64".into()))?;
    let pubkey_bytes = base64::engine::general_purpose::STANDARD
        .decode(pubkey_b64)
        .map_err(|_| VotingError::BadRequest("invalid terra pubkey base64".into()))?;

    let amino = adr36_sign_doc_json(expected_address, message);
    let hash = Sha256::digest(amino.as_bytes());
    verify_prehash(&pubkey_bytes, &hash, &sig_bytes)?;

    let vk = VerifyingKey::from_sec1_bytes(&pubkey_bytes)
        .map_err(|_| VotingError::Unauthorized("invalid terra pubkey".into()))?;
    let compressed = vk.to_encoded_point(true);
    let derived = cosmos_address_from_pubkey(compressed.as_bytes(), hrp)?;
    if derived != expected_address {
        return Err(VotingError::Unauthorized(
            "terra pubkey does not match address".into(),
        ));
    }
    Ok(())
}

/// Fallback used only by Simulated Wallet tests that sign raw bytes (Legal-style).
/// Production Keplr path is [`verify_adr36`].
pub fn verify_terra_raw_fallback(
    expected_address: &str,
    message: &str,
    signature_b64: &str,
    pubkey_b64: &str,
) -> VotingResult<()> {
    let hrp = expected_address
        .split_once('1')
        .map(|(p, _)| p)
        .ok_or_else(|| VotingError::BadRequest("invalid bech32 address".into()))?;
    let sig_bytes = base64::engine::general_purpose::STANDARD
        .decode(signature_b64)
        .map_err(|_| VotingError::BadRequest("invalid terra signature base64".into()))?;
    let pubkey_bytes = base64::engine::general_purpose::STANDARD
        .decode(pubkey_b64)
        .map_err(|_| VotingError::BadRequest("invalid terra pubkey base64".into()))?;
    let vk = VerifyingKey::from_sec1_bytes(&pubkey_bytes)
        .map_err(|_| VotingError::Unauthorized("invalid terra pubkey".into()))?;
    let sig = Signature::from_slice(&sig_bytes)
        .map_err(|_| VotingError::Unauthorized("invalid terra signature".into()))?;
    vk.verify(message.as_bytes(), &sig)
        .map_err(|_| VotingError::Unauthorized("terra signature verification failed".into()))?;
    let compressed = vk.to_encoded_point(true);
    let derived = cosmos_address_from_pubkey(compressed.as_bytes(), hrp)?;
    if derived != expected_address {
        return Err(VotingError::Unauthorized(
            "terra pubkey does not match address".into(),
        ));
    }
    Ok(())
}

pub fn verify_terra(
    expected_address: &str,
    message: &str,
    signature_b64: &str,
    pubkey_b64: &str,
) -> VotingResult<()> {
    match verify_adr36(expected_address, message, signature_b64, pubkey_b64) {
        Ok(()) => Ok(()),
        Err(_) => verify_terra_raw_fallback(expected_address, message, signature_b64, pubkey_b64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::ecdsa::{signature::hazmat::PrehashSigner, signature::Signer, SigningKey};

    fn key() -> SigningKey {
        SigningKey::from_bytes((&[0x33u8; 32]).into()).expect("key")
    }

    fn address_of(sk: &SigningKey) -> (String, String) {
        let vk = VerifyingKey::from(sk);
        let compressed = vk.to_encoded_point(true);
        let address = cosmos_address_from_pubkey(compressed.as_bytes(), "terra").unwrap();
        let pubkey_b64 = base64::engine::general_purpose::STANDARD.encode(compressed.as_bytes());
        (address, pubkey_b64)
    }

    #[test]
    fn adr36_roundtrip() {
        let sk = key();
        let (address, pubkey_b64) = address_of(&sk);
        let message = r#"{"app":"cl8y-voting","purpose":"register"}"#;
        let amino = adr36_sign_doc_json(&address, message);
        let hash = Sha256::digest(amino.as_bytes());
        let sig: k256::ecdsa::Signature = sk.sign_prehash(&hash).expect("sign");
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
        verify_adr36(&address, message, &sig_b64, &pubkey_b64).unwrap();
    }

    #[test]
    fn raw_fallback_roundtrip() {
        let sk = key();
        let (address, pubkey_b64) = address_of(&sk);
        let message = "CL8Y Terra test";
        let sig: Signature = sk.sign(message.as_bytes());
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
        verify_terra(&address, message, &sig_b64, &pubkey_b64).unwrap();
    }

    #[test]
    fn rejects_wrong_purpose_blob_as_different_message() {
        let sk = key();
        let (address, pubkey_b64) = address_of(&sk);
        let message = "register";
        let sig: Signature = sk.sign(message.as_bytes());
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
        let err = verify_terra(&address, "vote", &sig_b64, &pubkey_b64).unwrap_err();
        assert!(matches!(err, VotingError::Unauthorized(_)));
    }
}
