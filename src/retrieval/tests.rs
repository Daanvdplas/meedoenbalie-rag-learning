use super::*;
use crate::Section;

fn fixture_sections() -> Vec<Section> {
    crate::source::parse_sources(crate::DOCUMENTS)
        .unwrap()
        .into_iter()
        .flat_map(|source| source.sections)
        .collect()
}

fn identity(dimensions: usize) -> EmbeddingIdentity {
    EmbeddingIdentity {
        provider: "local-test-vectors".into(),
        model: "explicit-two-dimensional-fixture".into(),
        sdk: "test".into(),
        dimensions,
        encoding: "title-heading-text".into(),
    }
}

#[test]
fn activity_records_normalize_only_explicit_facts_and_keep_source_evidence() {
    let mut sections = fixture_sections();
    for section in &mut sections {
        if section.source_id == "ACT-FIETS-002" {
            section.source_id = "RENAMED-ACTIVITY".into();
        }
    }
    let corpus = PreparedCorpus::new(sections, None, None).unwrap();
    assert_eq!(corpus.activities.len(), 5);
    let bicycle = corpus
        .activities
        .iter()
        .find(|activity| activity.id == "RENAMED-ACTIVITY")
        .unwrap();
    assert_eq!(bicycle.section_ids.len(), 6);
    let schedule = bicycle.schedule.as_ref().unwrap();
    assert_eq!(schedule.value.weekdays, [Weekday::Tuesday]);
    assert_eq!(schedule.value.time.start_minutes, 14 * 60);
    assert_eq!(schedule.value.time.end_minutes, 16 * 60);
    assert_eq!(
        bicycle.environment.as_ref().unwrap().value,
        Environment::Indoor
    );
    for evidence in schedule
        .evidence
        .iter()
        .chain(&bicycle.environment.as_ref().unwrap().evidence)
    {
        assert!(corpus.sections.iter().any(|section| {
            section.source_id == evidence.source_id
                && section.id == evidence.section_id
                && section.source_date == evidence.source_date
                && section.text.contains(&evidence.quote)
        }));
    }
    assert!(corpus.identity.vectors_sha256.is_none());
    corpus.validate().unwrap();
}

#[test]
fn unknown_or_conflicting_facts_stay_unknown() {
    let sections = vec![
        section("A", "S1", "Binnen is niet beschreven. Misschien dinsdag."),
        section("B", "S1", "We werken binnen."),
        section("B", "S2", "Je werkt buiten."),
    ];
    let corpus = PreparedCorpus::new(sections, None, None).unwrap();
    assert!(
        corpus
            .activities
            .iter()
            .all(|activity| activity.schedule.is_none())
    );
    assert!(
        corpus
            .activities
            .iter()
            .all(|activity| activity.environment.is_none())
    );
}

#[test]
fn corpus_rejects_duplicate_identity_invalid_vectors_and_changed_provenance() {
    let sections = vec![section("A", "S1", "Exact source text.")];
    assert!(
        PreparedCorpus::new(vec![sections[0].clone(), sections[0].clone()], None, None).is_err()
    );
    assert!(PreparedCorpus::new(sections.clone(), Some(vec![vec![1.0, 0.0]]), None).is_err());
    assert!(
        PreparedCorpus::new(sections.clone(), Some(vec![vec![1.0]]), Some(identity(2))).is_err()
    );
    assert!(
        PreparedCorpus::new(
            sections.clone(),
            Some(vec![vec![0.0, 0.0]]),
            Some(identity(2))
        )
        .is_err()
    );
    let mut corpus =
        PreparedCorpus::new(sections, Some(vec![vec![1.0, 0.0]]), Some(identity(2))).unwrap();
    assert!(corpus.identity.vectors_sha256.is_some());
    corpus.validate().unwrap();
    corpus.sections[0].text.push_str(" Changed after indexing.");
    assert!(corpus.validate().is_err());
}

fn section(source: &str, id: &str, text: &str) -> Section {
    Section {
        source_id: source.into(),
        source_date: "2026-10-20".into(),
        source_title: format!("Activity {source}"),
        id: id.into(),
        heading: "Details".into(),
        text: text.into(),
    }
}
