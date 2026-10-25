//! Checking that a release was signed with the Probe release key.
//!
//! The private key never touches GitHub: the maintainer signs each release's
//! `SHA256SUMS` on their own machine with minisign
//! (`.github/scripts/sign-release.sh`). Only the public half is compiled in,
//! so a takeover of the GitHub account or the CI pipeline cannot produce an
//! update this app accepts.

use minisign_verify::{PublicKey, Signature};

use super::UpdateError;

/// The Probe release public key, in minisign's `.pub` format.
const RELEASE_PUBLIC_KEY: &str = include_str!("release-key.pub");

/// The trusted comment a release's signature must carry. It binds the signed
/// checksums to one version, so an old signed release can't be re-published
/// under a newer tag to roll users back.
pub fn expected_comment(tag: &str) -> String {
    format!("probe {tag}")
}

/// Verify that `signature` (the release's `SHA256SUMS.minisig`) is a valid
/// signature over `sums` by the Probe release key, made for `tag`.
pub fn verify_sums(sums: &[u8], signature: &str, tag: &str) -> Result<(), UpdateError> {
    verify_with(RELEASE_PUBLIC_KEY, sums, signature, tag)
}

fn verify_with(
    public_key: &str,
    sums: &[u8],
    signature: &str,
    tag: &str,
) -> Result<(), UpdateError> {
    let key = PublicKey::decode(public_key)
        .map_err(|error| UpdateError::Integrity(format!("release key unusable: {error}")))?;
    let signature = Signature::decode(signature)
        .map_err(|error| UpdateError::Integrity(format!("malformed signature: {error}")))?;
    // `false`: only accept minisign's current pre-hashed signatures.
    key.verify(sums, &signature, false)
        .map_err(|error| UpdateError::Integrity(format!("signature rejected: {error}")))?;

    // The comment is covered by the signature verified above, so it can be
    // trusted; it just has to name this release.
    let expected = expected_comment(tag);
    if signature.trusted_comment() != expected {
        return Err(UpdateError::Integrity(format!(
            "signature is for \"{}\", not \"{expected}\"",
            signature.trusted_comment()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Throwaway test key (Ed25519 seed of 32 × 0x01, key id 0102030405060708)
    // and signatures made with it in minisign's format. Never a real key.
    const TEST_KEY: &str = "untrusted comment: minisign public key TEST ONLY\n\
        RWQBAgMEBQYHCIqI4910CfGV/VLbLTy6XXLKZwm/HZQSG/N0iAG0D29c\n";
    const SIG_V999: &str = "untrusted comment: signature from minisign secret key\n\
        RUQBAgMEBQYHCHU0uhkR7YiGg8I3n3TLLI/lwPbsyygJh4TW5JdQ2Jw87hOJMQi7ZrDMdX5O6TcBbq5HhZ6B1/XTeeNSTmIoBAw=\n\
        trusted comment: probe v9.9.9\n\
        KxxprajCa8N8uXn0j5jkMP4k4LfHtcACUzrpi4PkNWXK1yuz/xhDERLixcb9OmS1z2DwxemnWstUPy7tq6vuCw==\n";
    const SIG_V100: &str = "untrusted comment: signature from minisign secret key\n\
        RUQBAgMEBQYHCHU0uhkR7YiGg8I3n3TLLI/lwPbsyygJh4TW5JdQ2Jw87hOJMQi7ZrDMdX5O6TcBbq5HhZ6B1/XTeeNSTmIoBAw=\n\
        trusted comment: probe v1.0.0\n\
        3StYs0JO8vwnBqxM4fkFKWSXAM+XJGDrqax3P6VklIWEV9uzzDdww85WGl5v+rNDpw4+YNMKDiLYVBhAOWReCA==\n";
    // Same checksums and comment, signed with a different key.
    const SIG_OTHER_KEY: &str = "untrusted comment: signature from minisign secret key\n\
        RUQREhMUFRYXGPAu4oxdiQ9CvkRk15+6UTcGusdteLe/WWd8vY8bAbMgrBbDbNHNgE9iroSTvdc5XEmokmBoExLFlFguo9ZYWA4=\n\
        trusted comment: probe v9.9.9\n\
        Wn6EvqDYKUoDQZGUSe7CEUe2qwtUe5yKOo0ddPqpYvjbQZ+9FV4ExqJNN9KL+9XTkk2Y8uFjN5OnY7BxQyOFBg==\n";

    fn sums() -> String {
        format!(
            "{}  probe-linux-x86_64\n{}  probe-windows-x86_64.exe\n",
            "a".repeat(64),
            "b".repeat(64)
        )
    }

    fn assert_integrity(result: Result<(), UpdateError>, needle: &str) {
        match result {
            Err(UpdateError::Integrity(message)) => {
                assert!(message.contains(needle), "{message}")
            }
            other => panic!("expected an integrity error containing {needle:?}, got {other:?}"),
        }
    }

    #[test]
    fn accepts_the_release_signed_for_this_tag() {
        verify_with(TEST_KEY, sums().as_bytes(), SIG_V999, "v9.9.9").unwrap();
    }

    #[test]
    fn rejects_tampered_checksums() {
        let tampered = sums().replace(&"a".repeat(64), &"c".repeat(64));
        assert_integrity(
            verify_with(TEST_KEY, tampered.as_bytes(), SIG_V999, "v9.9.9"),
            "signature rejected",
        );
    }

    #[test]
    fn rejects_a_signature_from_another_key() {
        assert_integrity(
            verify_with(TEST_KEY, sums().as_bytes(), SIG_OTHER_KEY, "v9.9.9"),
            "signature rejected",
        );
    }

    #[test]
    fn rejects_an_old_signed_release_under_a_newer_tag() {
        // Genuinely signed, but for v1.0.0: replaying it as v9.9.9 is a rollback.
        assert_integrity(
            verify_with(TEST_KEY, sums().as_bytes(), SIG_V100, "v9.9.9"),
            "not \"probe v9.9.9\"",
        );
    }

    #[test]
    fn rejects_an_edited_trusted_comment() {
        let edited = SIG_V100.replace("probe v1.0.0", "probe v9.9.9");
        assert_integrity(
            verify_with(TEST_KEY, sums().as_bytes(), &edited, "v9.9.9"),
            "signature rejected",
        );
    }

    #[test]
    fn rejects_garbage_signatures_and_keys() {
        assert_integrity(
            verify_with(TEST_KEY, sums().as_bytes(), "not a signature", "v9.9.9"),
            "malformed signature",
        );
        assert_integrity(
            verify_with("not\na key\n", sums().as_bytes(), SIG_V999, "v9.9.9"),
            "release key unusable",
        );
    }

    #[test]
    fn embedded_release_key_is_a_valid_minisign_key() {
        // A broken or placeholder key would make every update fail verification.
        PublicKey::decode(RELEASE_PUBLIC_KEY).unwrap();
    }

    #[test]
    fn embedded_key_is_never_the_test_key() {
        assert_ne!(RELEASE_PUBLIC_KEY.lines().nth(1), TEST_KEY.lines().nth(1));
    }
}
