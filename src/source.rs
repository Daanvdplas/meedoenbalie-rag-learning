use crate::Section;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Source {
    pub id: String,
    pub title: String,
    pub date: String,
    pub sections: Vec<Section>,
}
pub fn parse_source(text: &str) -> Result<Source, String> {
    let mut id = None;
    let mut date = None;
    for line in text.lines().take_while(|line| !line.starts_with("## ")) {
        if let Some(value) = line.strip_prefix("ID: ")
            && id.replace(value.trim().to_string()).is_some()
        {
            return Err("Duplicate document ID".into());
        }
        if let Some(value) = line.strip_prefix("Brondatum: ")
            && date.replace(value.trim().to_string()).is_some()
        {
            return Err("Duplicate source date".into());
        }
    }
    let id = id
        .filter(|value| !value.is_empty())
        .ok_or("Missing document ID")?;
    let date = date.ok_or("Missing source date")?;
    validate_date(&date)?;
    let title = text
        .lines()
        .find_map(|line| line.strip_prefix("# "))
        .filter(|s| !s.trim().is_empty())
        .ok_or("Missing document title")?
        .to_owned();
    let mut sections = crate::sections(text)?;
    for section in &mut sections {
        section.source_id = id.clone();
        section.source_date = date.clone();
        section.source_title = title.clone();
    }
    Ok(Source {
        id,
        title,
        date,
        sections,
    })
}
pub fn parse_sources(documents: &[&str]) -> Result<Vec<Source>, String> {
    if documents.len() > 5 {
        return Err("This experiment supports at most five activity documents".into());
    }
    let mut sources = Vec::new();
    for document in documents {
        let source = parse_source(document)?;
        if sources.iter().any(|s: &Source| s.id == source.id) {
            return Err("Duplicate document ID in corpus".into());
        }
        sources.push(source);
    }
    if sources.is_empty() {
        return Err("Corpus must contain documents".into());
    }
    Ok(sources)
}
pub fn validate_date(date: &str) -> Result<(), String> {
    let bytes = date.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return Err("Expected date YYYY-MM-DD".into());
    }
    let year: u32 = date[..4].parse().map_err(|_| "Invalid year")?;
    let month: usize = date[5..7].parse().map_err(|_| "Invalid month")?;
    let day: u32 = date[8..].parse().map_err(|_| "Invalid day")?;
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if year == 0 || !(1..=12).contains(&month) || day == 0 || day > days[month - 1] {
        return Err("Invalid calendar date".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const VALID: &str = "# Title\nID: DOC-A\nBrondatum: 2026-10-20\n\n## S1 — When\nEvery Tuesday.\n\nAn exact second paragraph.\n\n## S6 — Places\nTwo places on 2026-09-01; not checked since.\n";
    #[test]
    fn small_corpus_rejects_more_than_five_documents_before_indexing() {
        let texts: Vec<_> = (0..6)
            .map(|n| VALID.replace("DOC-A", &format!("DOC-{n}")))
            .collect();
        let refs: Vec<_> = texts.iter().map(String::as_str).collect();
        assert!(parse_sources(&refs).is_err());
    }
    #[test]
    fn default_corpus_has_exactly_five_distinct_activity_documents() {
        let sources = parse_sources(crate::DOCUMENTS).unwrap();
        assert_eq!(sources.len(), 5);
        let ids: std::collections::HashSet<_> = sources.iter().map(|source| &source.id).collect();
        assert_eq!(ids.len(), 5);
        assert_eq!(
            sources
                .iter()
                .map(|source| source.sections.len())
                .sum::<usize>(),
            30
        );
        for source in sources {
            assert!(source.sections.iter().all(
                |section| section.source_id == source.id && section.source_date == source.date
            ));
        }
    }
    #[test]
    fn corpus_allows_repeated_section_ids_but_rejects_duplicate_document_ids() {
        let other = VALID.replace("DOC-A", "DOC-B");
        assert_eq!(parse_sources(&[VALID, &other]).unwrap().len(), 2);
        assert!(parse_sources(&[VALID, VALID]).is_err());
        assert!(parse_sources(&[]).is_err());
    }
    #[test]
    fn every_section_retains_its_own_document_identity_date_and_title() {
        let a = parse_source(VALID).unwrap();
        let b = parse_source(
            &VALID
                .replace("DOC-A", "DOC-B")
                .replace("2026-10-20", "2026-10-18"),
        )
        .unwrap();
        let a = serde_json::to_value(a).unwrap();
        let b = serde_json::to_value(b).unwrap();
        assert_eq!(a["sections"][0]["source_id"], "DOC-A");
        assert_eq!(b["sections"][0]["source_id"], "DOC-B");
        assert_eq!(b["sections"][0]["source_date"], "2026-10-18");
        assert_eq!(b["sections"][0]["source_title"], "Title");
        assert_eq!(a["sections"][0]["id"], b["sections"][0]["id"]);
    }
    #[test]
    fn provenance_comes_from_source_and_keeps_paragraphs() {
        let source = parse_source(VALID).unwrap();
        assert_eq!(source.id, "DOC-A");
        assert_eq!(source.date, "2026-10-20");
        assert_eq!(
            source.sections[0].text,
            "Every Tuesday.\n\nAn exact second paragraph."
        );
        assert_eq!(source.sections[1].id, "S6");
        assert_eq!(source.sections.len(), 2);
    }
    #[test]
    fn rejects_missing_or_ambiguous_metadata_and_invalid_calendar_dates() {
        for invalid in [
            VALID.replace("ID: DOC-A\n", ""),
            VALID.replace("Brondatum: 2026-10-20\n", ""),
            VALID.replace("ID: DOC-A", "ID: DOC-A\nID: DOC-B"),
            VALID.replace("2026-10-20", "2026-02-30"),
            VALID.replace("2026-10-20", "20-10-2026"),
            VALID.replace("## S6 — Places", "## S1 — Places"),
            VALID.replace("## S1 — When", "##  — When"),
            VALID.replace("Every Tuesday.\n\nAn exact second paragraph.", ""),
        ] {
            assert!(
                parse_source(&invalid).is_err(),
                "accepted invalid source: {invalid}"
            );
        }
    }
    #[test]
    fn metadata_must_come_before_sections() {
        assert!(parse_source("# Title\n## S1 — Body\nID: DOC-A\nBrondatum: 2026-10-20").is_err());
    }
}
