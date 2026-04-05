//! Password hashing and verification using argon2id
//!
//! Uses the argon2 crate with OWASP-recommended parameters:
//! - m=65536 (64 MiB memory)
//! - t=3 (3 iterations)
//! - p=4 (4 lanes)

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

/// Hash a password using argon2id
///
/// Returns the encoded hash string (PHC format) that includes the salt,
/// parameters, and hash. This string can be stored directly in the database.
pub fn hash_password(password: &str) -> Result<String, password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2.hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

/// Verify a password against an argon2id hash
///
/// Returns `true` if the password matches the hash, `false` otherwise.
/// Returns an error if the hash format is invalid.
pub fn verify_password(password: &str, hash: &str) -> Result<bool, password_hash::Error> {
    let parsed_hash = PasswordHash::new(hash)?;
    let argon2 = Argon2::default();
    Ok(argon2.verify_password(password.as_bytes(), &parsed_hash).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_password_format() {
        let hash = hash_password("password123").unwrap();
        assert!(
            hash.starts_with("$argon2id$"),
            "Hash must use argon2id PHC format"
        );
    }

    #[test]
    fn test_verify_correct_password() {
        let hash = hash_password("password123").unwrap();
        assert!(verify_password("password123", &hash).unwrap());
    }

    #[test]
    fn test_verify_wrong_password() {
        let hash = hash_password("password123").unwrap();
        assert!(!verify_password("wrongpassword", &hash).unwrap());
    }

    #[test]
    fn test_different_salts_produce_different_hashes() {
        let hash1 = hash_password("password123").unwrap();
        let hash2 = hash_password("password123").unwrap();
        assert_ne!(hash1, hash2, "Same password should produce different hashes due to random salt");
    }

    #[test]
    fn test_empty_password() {
        let hash = hash_password("").unwrap();
        assert!(verify_password("", &hash).unwrap());
        assert!(!verify_password("notempty", &hash).unwrap());
    }

    #[test]
    fn test_long_password() {
        let password = "a".repeat(1000);
        let hash = hash_password(&password).unwrap();
        assert!(verify_password(&password, &hash).unwrap());
    }
}
