use oauth2::{PkceCodeChallenge, PkceCodeVerifier};

pub(crate) fn generate() -> (PkceCodeChallenge, PkceCodeVerifier) {
    PkceCodeChallenge::new_random_sha256()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_non_empty_pair() {
        let (challenge, verifier) = generate();
        assert!(!challenge.as_str().is_empty());
        assert!(!verifier.secret().is_empty());
    }

    #[test]
    fn generate_produces_fresh_values_each_call() {
        let (c1, v1) = generate();
        let (c2, v2) = generate();
        assert_ne!(c1.as_str(), c2.as_str());
        assert_ne!(v1.secret(), v2.secret());
    }
}
