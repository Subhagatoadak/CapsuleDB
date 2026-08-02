//! Tiny hex codec — avoids pulling in the `hex` crate for the handful of
//! byte arrays (signatures, public/secret keys) this crate needs to
//! stringify.

pub fn encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn decode(s: &str) -> Result<Vec<u8>, String> {
    if !s.len().is_multiple_of(2) {
        return Err("hex string has odd length".to_string());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| format!("invalid hex byte '{}': {e}", &s[i..i + 2])))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let bytes = [0u8, 1, 15, 16, 255];
        let encoded = encode(&bytes);
        assert_eq!(encoded, "00010f10ff");
        assert_eq!(decode(&encoded).unwrap(), bytes);
    }

    #[test]
    fn rejects_odd_length() {
        assert!(decode("abc").is_err());
    }

    #[test]
    fn rejects_non_hex() {
        assert!(decode("zz").is_err());
    }
}
