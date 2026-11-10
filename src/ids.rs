//! Short random typeable Artifact ids.

/// Lowercase letters and digits without look-alikes (no 0, o, 1, l, i).
pub const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
pub const ID_LEN: usize = 8;

pub fn new_artifact_id() -> Result<String, crate::error::Error> {
    // Rejection sampling keeps the distribution uniform over the alphabet.
    let limit = 256 - (256 % ALPHABET.len());
    let mut id = String::with_capacity(ID_LEN);
    let mut buf = [0u8; 32];
    while id.len() < ID_LEN {
        getrandom::fill(&mut buf)
            .map_err(|e| crate::error::Error::Unexpected(format!("no randomness: {e}")))?;
        for b in buf {
            if (b as usize) < limit && id.len() < ID_LEN {
                id.push(ALPHABET[b as usize % ALPHABET.len()] as char);
            }
        }
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_shape() {
        for _ in 0..200 {
            let id = new_artifact_id().unwrap();
            assert_eq!(id.len(), ID_LEN);
            assert!(id.bytes().all(|b| ALPHABET.contains(&b)));
        }
        assert!(!ALPHABET.iter().any(|b| b"0o1li".contains(b)));
    }
}
