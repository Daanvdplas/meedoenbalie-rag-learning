use serde::Serialize;
#[derive(Debug, Clone, Serialize)]
pub struct SourceFingerprint {
    pub id: String,
    pub title: String,
    pub date: String,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Manifest {
    pub sources: Vec<SourceFingerprint>,
    pub source_sha256: String,
    pub embedding_inputs_sha256: String,
    pub document_vectors_sha256: String,
    pub model: &'static str,
    pub dimensions: usize,
    pub sdk: &'static str,
    pub settings_sha256: String,
    pub chunking: &'static str,
    pub encoding: &'static str,
    pub max_tokens: usize,
    pub filter: &'static str,
    pub as_of: &'static str,
    pub platform: String,
}
pub fn hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_fingerprint_detects_even_small_evidence_changes() {
        assert_ne!(
            hash(b"Two places on September 1"),
            hash(b"Two places today")
        );
        assert_eq!(hash(b"source"), hash(b"source"));
        assert_eq!(hash(b"source").len(), 64);
    }
}
