pub mod codex;
pub mod manifest;
pub mod mcp;
pub mod ranking;
pub mod runs;
pub mod source;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Section {
    pub id: String,
    pub heading: String,
    pub text: String,
}
pub fn sections(document: &str) -> Result<Vec<Section>, String> {
    let mut result: Vec<Section> = Vec::new();
    for line in document.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            let (id, title) = heading.split_once(" — ").ok_or("Invalid section heading")?;
            if !id.starts_with('S')
                || id.len() < 2
                || !id[1..].chars().all(|c| c.is_ascii_digit())
                || title.trim().is_empty()
            {
                return Err("Invalid section ID or title".into());
            }
            if result.iter().any(|section| section.id == id) {
                return Err("Duplicate section ID".into());
            }
            result.push(Section {
                id: id.into(),
                heading: title.into(),
                text: String::new(),
            });
        } else if let Some(section) = result.last_mut() {
            section.text.push_str(line);
            section.text.push('\n');
        }
    }
    for section in &mut result {
        section.text = section.text.trim().into();
    }
    if result.is_empty() || result.iter().any(|s| s.text.is_empty()) {
        return Err("Document must contain nonempty sections".into());
    }
    Ok(result)
}
pub fn validate_query(query: &str) -> Result<String, String> {
    let query = query.trim();
    if query.is_empty() || query.chars().count() > 1000 {
        return Err("Please enter a question of 1 to 1000 characters.".into());
    }
    Ok(query.into())
}

pub const MODEL: &str = "Qdrant/paraphrase-multilingual-MiniLM-L12-v2-onnx-Q";
pub const DOCUMENT: &str = include_str!("../fixtures/document.md");

#[derive(Serialize)]
pub struct Hit {
    pub section: Section,
    pub similarity: f32,
}
#[derive(Serialize)]
pub struct SearchResult {
    pub question: String,
    pub model: &'static str,
    pub source_id: String,
    pub source_date: String,
    pub as_of: &'static str,
    pub hits: Vec<Hit>,
    pub manifest: manifest::Manifest,
    pub top_k: usize,
}

pub const AS_OF: &str = "2026-10-20";
pub const DEFAULT_TOP_K: usize = 3;
pub struct Index {
    model: fastembed::TextEmbedding,
    source: source::Source,
    vectors: Vec<Vec<f32>>,
    manifest: manifest::Manifest,
}
impl Index {
    pub fn load() -> Result<Self, String> {
        let source = source::parse_source(DOCUMENT)?;
        eprintln!("Loading local multilingual model (first run downloads model files)...");
        let mut model = fastembed::TextEmbedding::try_new(
            fastembed::TextInitOptions::new(fastembed::EmbeddingModel::ParaphraseMLMiniLML12V2Q)
                .with_cache_dir(std::path::PathBuf::from(".model-cache"))
                .with_show_download_progress(true)
                .with_max_length(512)
                .with_intra_threads(2),
        )
        .map_err(|e| e.to_string())?;
        let texts: Vec<_> = source
            .sections
            .iter()
            .map(|s| format!("{}\n{}", s.heading, s.text))
            .collect();
        let vectors = model.embed(texts, None).map_err(|e| e.to_string())?;
        if vectors.len() != source.sections.len() || vectors.iter().any(|v| v.len() != 384) {
            return Err("Embedding count or dimensions do not match the source/model".into());
        }
        for vector in &vectors {
            ranking::cosine(vector, vector)?;
        }
        let vector_bytes: Vec<_> = vectors
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let manifest = manifest::Manifest {
            source_sha256: manifest::hash(DOCUMENT.as_bytes()),
            document_vectors_sha256: manifest::hash(&vector_bytes),
            model: MODEL, dimensions:384, sdk:"fastembed 7.1.2",
            settings_sha256: manifest::hash(format!("{MODEL}|384|512|fastembed7.1.2|sections-v1|unprefixed-heading-newline-text|cosine|single-document|{AS_OF}").as_bytes()),
            chunking:"section-headings-v1",encoding:"unprefixed sentence transformer; heading + newline + text for sections",max_tokens:512,
            filter:"single fictional document; no eligibility inference",as_of:AS_OF,
            platform:format!("{}-{}",std::env::consts::OS,std::env::consts::ARCH),
        };
        Ok(Self {
            model,
            source,
            vectors,
            manifest,
        })
    }
    pub fn source(&self) -> &source::Source {
        &self.source
    }
    pub fn manifest(&self) -> &manifest::Manifest {
        &self.manifest
    }
    pub fn search(&mut self, question: &str) -> Result<SearchResult, String> {
        self.search_with_top_k(question, DEFAULT_TOP_K)
    }
    pub fn search_with_top_k(
        &mut self,
        question: &str,
        top_k: usize,
    ) -> Result<SearchResult, String> {
        let question = validate_query(question)?;
        ranking::validate_top_k(top_k)?;
        let mut embedded = self
            .model
            .embed([&question], None)
            .map_err(|e| e.to_string())?;
        if embedded.len() != 1 {
            return Err("Expected one query embedding".into());
        }
        let query = embedded.remove(0);
        let scores = self
            .vectors
            .iter()
            .map(|vector| ranking::cosine(&query, vector))
            .collect::<Result<Vec<_>, _>>()?;
        let hits = ranking::rank(&self.source.sections, &scores, top_k)?;
        Ok(SearchResult {
            question,
            model: MODEL,
            source_id: self.source.id.clone(),
            source_date: self.source.date.clone(),
            as_of: AS_OF,
            hits,
            manifest: self.manifest.clone(),
            top_k,
        })
    }
}

pub fn keyword_search(
    source: &source::Source,
    question: &str,
    top_k: usize,
) -> Result<Vec<Hit>, String> {
    use std::collections::HashSet;
    fn tokens(text: &str) -> HashSet<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(str::to_owned)
            .collect()
    }
    let query = tokens(&validate_query(question)?);
    let scores: Vec<_> = source
        .sections
        .iter()
        .map(|section| {
            let words = tokens(&format!("{} {}", section.heading, section.text));
            query.intersection(&words).count() as f32 / query.union(&words).count() as f32
        })
        .collect();
    ranking::rank(&source.sections, &scores, top_k)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keyword_baseline_searches_arbitrary_text_and_does_not_claim_no_match() {
        let source = source::Source {
            id: "X".into(),
            date: "2026-10-20".into(),
            sections: vec![
                Section {
                    id: "S1".into(),
                    heading: "Tools".into(),
                    text: "Gloves and tools provided.".into(),
                },
                Section {
                    id: "S2".into(),
                    heading: "Time".into(),
                    text: "Tuesday morning.".into(),
                },
            ],
        };
        let hits = keyword_search(&source, "GLOVES", 1).unwrap();
        assert_eq!(hits[0].section.id, "S1");
        let absent = keyword_search(&source, "transport", 2).unwrap();
        assert_eq!(absent.len(), 2);
        assert!(absent.iter().all(|hit| hit.similarity == 0.0));
    }
    #[test]
    fn keeps_exact_evidence_and_does_not_invent_missing_sections() {
        let source = "# Example\nID: X\n\n## S1 — When\nEvery Tuesday.\n\n## S6 — Places\nTwo places in September.\n";
        let chunks = sections(source).unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].id, "S1");
        assert_eq!(chunks[0].text, "Every Tuesday.");
        assert_eq!(chunks[1].id, "S6");
        assert!(chunks.iter().all(|s| !s.text.contains("transport")));
    }
    #[test]
    fn rejects_duplicate_sections_instead_of_ambiguous_citations() {
        assert!(sections("## S1 — First\nOne\n## S1 — Second\nTwo").is_err());
    }
    #[test]
    fn rejects_empty_or_oversized_questions() {
        assert!(validate_query(" \n ").is_err());
        assert!(validate_query(&"a".repeat(1001)).is_err());
        assert_eq!(validate_query("  Wanneer?  ").unwrap(), "Wanneer?");
    }
}
