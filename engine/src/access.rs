use anyhow::{bail, Result};
#[cfg(feature = "wasm")]
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PartnerStatus {
    Active,
    Suspended,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartnerConfig {
    pub firm_name: String,
    pub logo_url: String,
    pub brand_color: String,
    pub disclaimer_text: String,
    pub status: PartnerStatus,
}

pub type AccessRegistry = HashMap<String, PartnerConfig>;

pub fn hash_code(code: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(code.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

#[cfg(feature = "wasm")]
pub async fn validate_access_code_inner(code: &str, registry_url: &str) -> Result<PartnerConfig> {
    // v2 change to force update
    let hashed_code = hash_code(code);

    let registry: AccessRegistry = Request::get(registry_url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to fetch registry: {}", e))?
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse registry JSON: {}", e))?;

    if let Some(config) = registry.get(&hashed_code) {
        match config.status {
            PartnerStatus::Active => Ok(config.clone()),
            PartnerStatus::Suspended => bail!("Partner access is suspended"),
        }
    } else {
        bail!("Invalid access code")
    }
}

#[cfg(not(feature = "wasm"))]
pub async fn validate_access_code_inner(_code: &str, _registry_url: &str) -> Result<PartnerConfig> {
    bail!("Access code validation is only supported in WASM target for now.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_code_empty() {
        let input = "";
        let expected = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(hash_code(input), expected);
    }

    #[test]
    fn test_hash_code_abc() {
        let input = "abc";
        let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(hash_code(input), expected);
    }

    #[test]
    fn test_hash_code_known_string() {
        let input = "hello world";
        let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        assert_eq!(hash_code(input), expected);
    }

    #[test]
    fn test_hash_code_unicode() {
        let input = "🦀 Rust";
        let expected = "b4ac1c68f49c3ea5a386adb82a04447992f218b5aeb19a001182d3722a399931";
        assert_eq!(hash_code(input), expected);
    }

    #[test]
    fn test_hash_code_case_sensitivity() {
        assert_ne!(hash_code("Code123"), hash_code("code123"));
    }

    #[test]
    fn test_hash_code_different_inputs() {
        assert_ne!(hash_code("partner_a"), hash_code("partner_b"));
    }

    #[test]
    fn test_hash_code_consistency() {
        let input = "some_random_code_123";
        assert_eq!(hash_code(input), hash_code(input));
    }

    #[test]
    fn test_hash_code_long_input() {
        let input = "a".repeat(1000);
        let expected = "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3";
        assert_eq!(hash_code(&input), expected);
    }
}
