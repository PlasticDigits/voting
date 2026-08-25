use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use sha3::{Digest, Keccak256};

use crate::blacklist::normalize_address;
use crate::error::{VotingError, VotingResult};

pub fn eip191_hash(message: &str) -> [u8; 32] {
    let prefix = format!("\x19Ethereum Signed Message:\n{}", message.len());
    let mut hasher = Keccak256::new();
    hasher.update(prefix.as_bytes());
    hasher.update(message.as_bytes());
    hasher.finalize().into()
}

pub fn recover_evm_address(message: &str, signature_hex: &str) -> VotingResult<String> {
    let sig_bytes = hex::decode(signature_hex.trim_start_matches("0x"))
        .map_err(|_| VotingError::BadRequest("invalid EVM signature encoding".into()))?;
    if sig_bytes.len() != 65 {
        return Err(VotingError::BadRequest("EVM signature must be 65 bytes".into()));
    }
    let v = sig_bytes[64];
    let rec_id = if v >= 27 {
        RecoveryId::from_byte(v - 27)
    } else {
        RecoveryId::from_byte(v)
    }
    .ok_or_else(|| VotingError::BadRequest("invalid recovery id".into()))?;
    let sig = Signature::from_slice(&sig_bytes[..64])
        .map_err(|_| VotingError::Unauthorized("invalid signature".into()))?;
    let hash = eip191_hash(message);
    let vk = VerifyingKey::recover_from_prehash(&hash, &sig, rec_id)
        .map_err(|_| VotingError::Unauthorized("signature recovery failed".into()))?;
    let pubkey_bytes = vk.to_encoded_point(false);
    let pubkey = pubkey_bytes.as_bytes();
    let mut hasher = Keccak256::new();
    hasher.update(&pubkey[1..]);
    let digest: [u8; 32] = hasher.finalize().into();
    Ok(format!("0x{}", hex::encode(&digest[12..])))
}

pub fn verify_eip191(expected_address: &str, message: &str, signature_hex: &str) -> VotingResult<()> {
    let recovered = recover_evm_address(message, signature_hex)?;
    if recovered != normalize_address(expected_address) {
        return Err(VotingError::Unauthorized(
            "signature does not match address".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::ecdsa::SigningKey;

    fn address_from_key(key: &SigningKey) -> String {
        let vk = VerifyingKey::from(key);
        let pubkey_bytes = vk.to_encoded_point(false);
        let pubkey = pubkey_bytes.as_bytes();
        let mut hasher = Keccak256::new();
        hasher.update(&pubkey[1..]);
        let digest: [u8; 32] = hasher.finalize().into();
        format!("0x{}", hex::encode(&digest[12..]))
    }

    fn sign(key: &SigningKey, msg: &str) -> String {
        let hash = eip191_hash(msg);
        let (sig, recid) = key.sign_prehash_recoverable(&hash).unwrap();
        let mut bytes = [0u8; 65];
        bytes[..64].copy_from_slice(&sig.to_bytes());
        bytes[64] = recid.to_byte() + 27;
        format!("0x{}", hex::encode(bytes))
    }

    #[test]
    fn roundtrip() {
        let key = SigningKey::from_slice(&[0x11u8; 32]).unwrap();
        let address = address_from_key(&key);
        let msg = r#"{"app":"cl8y-voting","purpose":"vote"}"#;
        verify_eip191(&address, msg, &sign(&key, msg)).unwrap();
    }

    #[test]
    fn rejects_terra_payload_as_evm() {
        let key = SigningKey::from_slice(&[0x11u8; 32]).unwrap();
        let address = address_from_key(&key);
        let err = verify_eip191(&address, "vote", "0x00").unwrap_err();
        assert!(matches!(err, VotingError::BadRequest(_)));
    }

    #[test]
    fn checksum_match() {
        let key = SigningKey::from_slice(&[0x11u8; 32]).unwrap();
        let address = address_from_key(&key);
        let mixed = format!("0x{}", &address[2..].to_ascii_uppercase());
        let msg = "hello";
        verify_eip191(&mixed, msg, &sign(&key, msg)).unwrap();
    }
}
