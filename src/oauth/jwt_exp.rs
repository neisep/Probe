//! Reading the `exp` claim out of a JWT access token.
//!
//! Token endpoints are supposed to return `expires_in`, but some omit it.
//! Guessing a lifetime then means Probe either renews far too often (when the
//! real token lives for a day) or attaches a dead token (when it lived for
//! five minutes). Most access tokens are JWTs and carry the authoritative
//! expiry in their `exp` claim, so read it before falling back to a guess.
//!
//! **This is not a security check.** The signature is never verified and the
//! claim is never used to decide whether a token is trustworthy — only to
//! schedule renewal. A forged `exp` can make Probe renew at the wrong time,
//! nothing more.

use base64::Engine;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};

/// The `exp` claim as a unix timestamp, or `None` when the token is not a
/// JWT, the payload is unreadable, or `exp` is absent or not a positive
/// number.
pub(crate) fn exp_claim(access_token: &str) -> Option<i64> {
    let payload = jwt_payload(access_token)?;
    let claims: serde_json::Value = serde_json::from_slice(&payload).ok()?;
    let exp = match claims.get("exp")? {
        serde_json::Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|seconds| seconds as i64))?,
        _ => return None,
    };
    (exp > 0).then_some(exp)
}

/// Decode the middle segment of a `header.payload.signature` triple.
fn jwt_payload(access_token: &str) -> Option<Vec<u8>> {
    let mut segments = access_token.split('.');
    let _header = segments.next()?;
    let payload = segments.next()?;
    let _signature = segments.next()?;
    if segments.next().is_some() || payload.is_empty() {
        return None;
    }
    // Spec says unpadded base64url, but padded tokens exist in the wild.
    URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| URL_SAFE.decode(payload))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a JWT-shaped string whose payload is `claims`.
    fn jwt(claims: &str) -> String {
        format!(
            "{}.{}.{}",
            URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#),
            URL_SAFE_NO_PAD.encode(claims.as_bytes()),
            "signature-not-verified"
        )
    }

    #[test]
    fn reads_exp_from_a_jwt() {
        assert_eq!(exp_claim(&jwt(r#"{"exp":1893456000}"#)), Some(1893456000));
    }

    #[test]
    fn reads_exp_alongside_other_claims() {
        let token = jwt(r#"{"sub":"svc","exp":1893456000,"scope":"read write"}"#);
        assert_eq!(exp_claim(&token), Some(1893456000));
    }

    #[test]
    fn accepts_a_padded_payload() {
        let claims = r#"{"exp":1893456000}"#;
        let token = format!(
            "{}.{}.{}",
            URL_SAFE.encode(br#"{"alg":"none"}"#),
            URL_SAFE.encode(claims.as_bytes()),
            "sig"
        );
        assert_eq!(exp_claim(&token), Some(1893456000));
    }

    #[test]
    fn floating_point_exp_is_truncated() {
        assert_eq!(exp_claim(&jwt(r#"{"exp":1893456000.9}"#)), Some(1893456000));
    }

    #[test]
    fn opaque_token_yields_nothing() {
        assert_eq!(exp_claim("2YotnFZFEjr1zCsicMWpAA"), None);
    }

    #[test]
    fn wrong_segment_count_yields_nothing() {
        assert_eq!(exp_claim("only.two"), None);
        assert_eq!(exp_claim("a.b.c.d"), None);
    }

    #[test]
    fn undecodable_payload_yields_nothing() {
        assert_eq!(exp_claim("header.!!!not-base64!!!.sig"), None);
    }

    #[test]
    fn missing_or_non_numeric_exp_yields_nothing() {
        assert_eq!(exp_claim(&jwt(r#"{"sub":"svc"}"#)), None);
        assert_eq!(exp_claim(&jwt(r#"{"exp":"soon"}"#)), None);
        assert_eq!(exp_claim(&jwt(r#"{"exp":null}"#)), None);
    }

    #[test]
    fn non_positive_exp_yields_nothing() {
        assert_eq!(exp_claim(&jwt(r#"{"exp":0}"#)), None);
        assert_eq!(exp_claim(&jwt(r#"{"exp":-5}"#)), None);
    }

    #[test]
    fn non_json_payload_yields_nothing() {
        assert_eq!(exp_claim(&jwt("not json at all")), None);
    }
}
