use meedoen_retrieval::{experiment::dataset, manifest::hash};

#[test]
fn heldout_intents_are_separate_from_known_pilot_families_and_match_the_frozen_manifest() {
    let (pilot, _) = dataset::validate_bundled().unwrap();
    let (questions, labels) = dataset::parse(
        include_str!("../evals/shared/datasets/legacy/heldout-questions.json"),
        include_str!("../evals/shared/datasets/legacy/heldout-expectations.json"),
    )
    .unwrap();
    assert_eq!(questions.cases.len(), 6);
    assert!(questions.cases.iter().all(|case| {
        case.split == "held_out"
            && !pilot
                .cases
                .iter()
                .any(|known| known.id == case.id || known.family == case.family)
    }));
    let freeze: serde_json::Value = serde_json::from_str(include_str!(
        "../evals/shared/datasets/legacy/heldout-freeze.json"
    ))
    .unwrap();
    assert_eq!(
        freeze["questions_sha256"],
        dataset::questions_hash(&questions).unwrap()
    );
    assert_eq!(
        freeze["expectations_sha256"],
        hash(&serde_json::to_vec(&labels).unwrap())
    );
    assert_eq!(freeze["live_execution_at_freeze"], false);
    assert_eq!(freeze["human_calibration"], false);
}
