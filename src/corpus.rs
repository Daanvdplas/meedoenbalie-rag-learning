//! Explicit source versions keep historical measurements reproducible.
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CorpusId {
    #[default]
    BaselineFive,
    ExpandedTen,
    DemoTwelve,
}

impl CorpusId {
    pub fn is_baseline(&self) -> bool {
        *self == Self::BaselineFive
    }

    pub fn documents(self) -> &'static [&'static str] {
        match self {
            Self::BaselineFive => crate::DOCUMENTS,
            Self::ExpandedTen => &EXPANDED_DOCUMENTS,
            Self::DemoTwelve => &DEMO_DOCUMENTS,
        }
    }

    pub fn as_of(self) -> &'static str {
        match self {
            Self::BaselineFive => crate::AS_OF,
            Self::ExpandedTen => "2026-11-05",
            Self::DemoTwelve => "2026-11-13",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::BaselineFive => "baseline-five",
            Self::ExpandedTen => "expanded-ten",
            Self::DemoTwelve => "demo-twelve",
        }
    }

    pub fn sources(self) -> Result<Vec<crate::source::Source>, String> {
        crate::source::parse_sources(self.documents())
    }

    pub fn sha256(self) -> String {
        crate::manifest::hash(self.documents().join("\n---document---\n").as_bytes())
    }
}

const EXPANDED_DOCUMENTS: [&str; 10] = [
    crate::DOCUMENTS[0],
    crate::DOCUMENTS[1],
    crate::DOCUMENTS[2],
    crate::DOCUMENTS[3],
    crate::DOCUMENTS[4],
    include_str!("../fixtures/expanded-10-20261105/activity-006.md"),
    include_str!("../fixtures/expanded-10-20261105/activity-007.md"),
    include_str!("../fixtures/expanded-10-20261105/activity-008.md"),
    include_str!("../fixtures/expanded-10-20261105/activity-009.md"),
    include_str!("../fixtures/expanded-10-20261105/activity-010.md"),
];

const DEMO_DOCUMENTS: [&str; 12] = [
    EXPANDED_DOCUMENTS[0],
    EXPANDED_DOCUMENTS[1],
    EXPANDED_DOCUMENTS[2],
    EXPANDED_DOCUMENTS[3],
    EXPANDED_DOCUMENTS[4],
    EXPANDED_DOCUMENTS[5],
    EXPANDED_DOCUMENTS[6],
    EXPANDED_DOCUMENTS[7],
    EXPANDED_DOCUMENTS[8],
    EXPANDED_DOCUMENTS[9],
    include_str!("../fixtures/demo-12-20261113/activity-011.md"),
    include_str!("../fixtures/demo-12-20261113/activity-012.md"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twelve_document_live_corpus_preserves_frozen_sources_and_correct_prompt() {
        let live: CorpusId = serde_json::from_str("\"demo-twelve\"")
            .expect("the live demo needs its own source version");
        let historical = CorpusId::ExpandedTen;
        assert_eq!(&live.documents()[..10], historical.documents());
        let freeze: serde_json::Value = serde_json::from_str(include_str!(
            "../evals/shared/datasets/expanded-10-20261105/FROZEN.json"
        ))
        .unwrap();
        assert_eq!(historical.sha256(), freeze["integrity"]["corpus_sha256"]);
        assert_ne!(live.sha256(), historical.sha256());
        let sources = live.sources().unwrap();
        assert!(
            sources[10..]
                .iter()
                .all(|source| source.date == live.as_of()
                    && source.sections.len() == 6
                    && source
                        .sections
                        .iter()
                        .all(|section| section.source_id == source.id
                            && section.source_date == source.date))
        );
        assert!(
            crate::codex::generation_prompt_for_corpus("Welke activiteit?", 3, live)
                .contains("TWELVE fictional Dutch activity documents")
        );
    }

    #[test]
    fn expanded_sources_preserve_baseline_bytes_and_keep_distinct_identity() {
        let baseline = CorpusId::BaselineFive;
        let expanded = CorpusId::ExpandedTen;
        assert_eq!(&expanded.documents()[..5], baseline.documents());
        assert_eq!(baseline.sources().unwrap().len(), 5);
        let sources = expanded.sources().unwrap();
        assert_eq!(sources.len(), 10);
        assert!(sources.iter().all(|source| source.sections.len() == 6));
        assert!(
            sources[5..]
                .iter()
                .all(|source| source.date == expanded.as_of())
        );
        assert_ne!(baseline.sha256(), expanded.sha256());
        assert_ne!(baseline.as_of(), expanded.as_of());
    }
}
