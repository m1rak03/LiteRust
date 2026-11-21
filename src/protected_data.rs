//! Metadata encryption bound to the current Windows account. Not protection from
//! malware already running as that account, nor encryption of the WebView cache.
use std::io;
const MAGIC: &[u8] = b"LiteRustDPAPI1\0";
pub fn seal(bytes: &[u8]) -> io::Result<Vec<u8>> {
    #[cfg(windows)]
    {
        let mut result = MAGIC.to_vec();
        result.extend(crate::vpn_platform::crypt(bytes, true)?);
        if result.len() as u64 > crate::storage::MAX_BROWSER_DATA_BYTES {
            return Err(io::Error::other(
                "Encrypted metadata exceeds profile size limit",
            ));
        }
        Ok(result)
    }
    #[cfg(not(windows))]
    {
        Ok(bytes.to_vec())
    }
}
pub(crate) fn is_current_format(bytes: &[u8]) -> bool {
    !cfg!(windows) || bytes.starts_with(MAGIC)
}

pub fn open(bytes: &[u8]) -> io::Result<Vec<u8>> {
    if let Some(payload) = bytes.strip_prefix(MAGIC) {
        #[cfg(windows)]
        {
            crate::vpn_platform::crypt(payload, false)
        }
        #[cfg(not(windows))]
        {
            let _ = payload;
            Err(io::Error::other(
                "Windows account is required to decrypt this profile",
            ))
        }
    } else {
        Ok(bytes.to_vec())
    } // migrate legacy JSON on the next save
}
pub fn migrate(bytes: &[u8]) -> io::Result<Vec<u8>> {
    if bytes.starts_with(MAGIC) {
        open(bytes)?; // validate before preserving ciphertext
        Ok(bytes.to_vec())
    } else {
        seal(bytes)
    }
}

/// Protect a historical copy without changing existing DPAPI envelopes.
/// Live loads still authenticate the envelope with `open`.
pub(crate) fn protect_backup(bytes: &[u8]) -> io::Result<Vec<u8>> {
    if bytes.starts_with(MAGIC) {
        Ok(bytes.to_vec())
    } else {
        seal(bytes)
    }
}
#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn account_bound_roundtrip_and_tamper_rejection() {
        let secret = b"private-history.example";
        let encoded = seal(secret).unwrap();
        assert!(!encoded.windows(secret.len()).any(|w| w == secret));
        assert_eq!(open(&encoded).unwrap(), secret);
        assert_eq!(migrate(&encoded).unwrap(), encoded);
        let mut corrupt = encoded;
        *corrupt.last_mut().unwrap() ^= 1;
        assert!(open(&corrupt).is_err());
    }
}
