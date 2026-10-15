use alloy::primitives::{keccak256, B256};

pub fn new_refresh_token() -> (String, B256) {
    let token = alloy::hex::encode(B256::random());
    let hash = hash_refresh_token(&token);
    (token, hash)
}

pub fn hash_refresh_token(token: &str) -> B256 {
    keccak256(token.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    // a refresh token is 64 hex characters, differs on every call, and its stored hash is derived from it
    #[test]
    fn test_new_refresh_token_is_random_and_hashed() {
        let (token, hash) = new_refresh_token();
        let (other_token, _) = new_refresh_token();

        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(token, other_token);
        assert_eq!(hash, hash_refresh_token(&token));
    }
}
