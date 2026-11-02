use serde::Serialize;
use std::collections::BTreeMap;

pub const CACHE_VERSION: &str = "embedding-cache-v1-f32-le";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CacheKey {
    pub identity_sha256: String,
    pub input_sha256: String,
    pub dimensions: usize,
}

impl CacheKey {
    pub fn new(identity: &impl Serialize, input: &str, dimensions: usize) -> Result<Self, String> {
        if dimensions == 0 {
            return Err("Embedding dimensions must be positive".into());
        }
        Ok(Self {
            identity_sha256: crate::manifest::hash(
                &serde_json::to_vec(&(CACHE_VERSION, identity)).map_err(|e| e.to_string())?,
            ),
            input_sha256: crate::manifest::hash(input.as_bytes()),
            dimensions,
        })
    }
}

#[derive(Debug, Default)]
pub struct EmbeddingCache {
    entries: BTreeMap<CacheKey, Vec<f32>>,
}

impl EmbeddingCache {
    pub fn get(&self, key: &CacheKey) -> Result<Option<Vec<f32>>, String> {
        self.entries
            .get(key)
            .map(|vector| {
                validate_vector(key, vector)?;
                Ok(vector.clone())
            })
            .transpose()
    }

    pub fn put(&mut self, key: &CacheKey, vector: &[f32]) -> Result<(), String> {
        validate_vector(key, vector)?;
        self.entries.insert(key.clone(), vector.to_vec());
        Ok(())
    }
}

pub(super) fn validate_vector(key: &CacheKey, vector: &[f32]) -> Result<(), String> {
    if key.dimensions != vector.len() {
        return Err("Cached embedding dimensions do not match its identity".into());
    }
    crate::ranking::cosine(vector, vector).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(input: &str, model: &str, dimensions: usize) -> CacheKey {
        CacheKey::new(
            &serde_json::json!({"provider":"local-fastembed", "model":model, "sdk":"7.1.2", "encoding":"title-heading-text-v2"}),
            input,
            dimensions,
        )
        .unwrap()
    }

    #[test]
    fn cache_reuses_only_identical_model_input_and_dimensions() {
        let mut cache = EmbeddingCache::default();
        let original = key("Fietsen\nWanneer\nDinsdag 14–16", "mini", 3);
        let vector = [0.25, -0.0, 0.75];
        cache.put(&original, &vector).unwrap();
        let actual = cache.get(&original).unwrap().unwrap();
        assert_eq!(
            actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            vector.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
        );
        for changed in [
            key("Fietsen\nWanneer\nWoensdag 14–16", "mini", 3),
            key("Fietsen\nWanneer\nDinsdag 14–16", "other", 3),
            key("Fietsen\nWanneer\nDinsdag 14–16", "mini", 2),
        ] {
            assert!(cache.get(&changed).unwrap().is_none());
        }
    }

    #[test]
    fn cache_rejects_invalid_vectors_instead_of_persisting_bad_values() {
        let mut cache = EmbeddingCache::default();
        let expected = key("text", "mini", 3);
        for invalid in [
            vec![1.0, 2.0],
            vec![0.0; 3],
            vec![f32::NAN, 1.0, 2.0],
            vec![f32::INFINITY, 1.0, 2.0],
        ] {
            assert!(cache.put(&expected, &invalid).is_err());
        }
        assert!(CacheKey::new(&"model", "text", 0).is_err());
    }
}
