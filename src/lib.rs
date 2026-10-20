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
    pub source_id: &'static str,
    pub source_date: &'static str,
    pub as_of: &'static str,
    pub hits: Vec<Hit>,
}

pub struct Index {
    model: fastembed::TextEmbedding,
    sections: Vec<Section>,
    vectors: Vec<Vec<f32>>,
}
impl Index {
    pub fn load() -> Result<Self, String> {
        let sections = sections(DOCUMENT)?;
        eprintln!("Loading local multilingual model (first run downloads model files)...");
        let mut model = fastembed::TextEmbedding::try_new(
            fastembed::TextInitOptions::new(fastembed::EmbeddingModel::ParaphraseMLMiniLML12V2Q)
                .with_cache_dir(std::path::PathBuf::from(".model-cache"))
                .with_show_download_progress(true)
                .with_intra_threads(2),
        )
        .map_err(|e| e.to_string())?;
        // This sentence-transformer model uses the same unprefixed encoding for both inputs.
        let texts: Vec<_> = sections
            .iter()
            .map(|s| format!("{}\n{}", s.heading, s.text))
            .collect();
        let vectors = model.embed(texts, None).map_err(|e| e.to_string())?;
        Ok(Self {
            model,
            sections,
            vectors,
        })
    }
    pub fn search(&mut self, question: &str) -> Result<SearchResult, String> {
        let question = validate_query(question)?;
        let query = self
            .model
            .embed([&question], None)
            .map_err(|e| e.to_string())?
            .remove(0);
        let mut hits: Vec<_> = self
            .sections
            .iter()
            .zip(&self.vectors)
            .map(|(section, vector)| {
                let dot: f32 = query.iter().zip(vector).map(|(a, b)| a * b).sum();
                let norm = query.iter().map(|v| v * v).sum::<f32>().sqrt()
                    * vector.iter().map(|v| v * v).sum::<f32>().sqrt();
                Hit {
                    section: section.clone(),
                    similarity: dot / norm,
                }
            })
            .collect();
        hits.sort_by(|a, b| {
            b.similarity
                .total_cmp(&a.similarity)
                .then(a.section.id.cmp(&b.section.id))
        });
        hits.truncate(3);
        Ok(SearchResult {
            question,
            model: MODEL,
            source_id: "ACT-TUIN-001",
            source_date: "2026-10-20",
            as_of: "2026-10-20",
            hits,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
