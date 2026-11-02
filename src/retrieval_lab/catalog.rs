use crate::{retrieval::SearchMethod, storage::StorageProfile};

use super::{
    dataset::bundled_questions,
    types::{ExperimentPlan, PlannedConfig, RunConfig},
};

pub const METHODS: [SearchMethod; 4] = [
    SearchMethod::Dense,
    SearchMethod::Bm25,
    SearchMethod::HybridRrf,
    SearchMethod::HybridWeighted,
];
pub const PROFILES: [StorageProfile; 4] = [
    StorageProfile::DocumentsSections,
    StorageProfile::DocumentsActivities,
    StorageProfile::SqliteSections,
    StorageProfile::SqliteActivities,
];

pub fn config_id(config: &RunConfig) -> Result<String, String> {
    let encoded = serde_json::to_vec(config).map_err(|error| error.to_string())?;
    let profile = serde_json::to_value(config.profile).map_err(|error| error.to_string())?;
    let method =
        serde_json::to_value(config.retrieval.method).map_err(|error| error.to_string())?;
    Ok(format!(
        "{}__{}__{}",
        profile.as_str().unwrap(),
        method.as_str().unwrap(),
        &crate::manifest::hash(&encoded)[..12]
    ))
}

pub fn plan(
    methods: &[SearchMethod],
    profiles: &[StorageProfile],
    case_ids: &[String],
    advanced: &crate::retrieval::RetrievalConfig,
) -> Result<ExperimentPlan, String> {
    if methods.is_empty()
        || profiles.is_empty()
        || methods
            .iter()
            .enumerate()
            .any(|(index, value)| methods[..index].contains(value))
        || profiles
            .iter()
            .enumerate()
            .any(|(index, value)| profiles[..index].contains(value))
    {
        return Err("Select distinct nonempty search methods and profiles".into());
    }
    crate::retrieval::validate_config(advanced, &Default::default())?;
    let catalog = bundled_questions()?;
    if case_ids.iter().enumerate().any(|(index, id)| {
        case_ids[..index].contains(id) || !catalog.cases.iter().any(|case| case.id == *id)
    }) {
        return Err("Unknown or duplicate question selection".into());
    }
    let cases: Vec<_> = catalog
        .cases
        .into_iter()
        .filter(|case| case_ids.is_empty() || case_ids.contains(&case.id))
        .collect();
    let mut configurations = Vec::new();
    for profile in profiles {
        for method in methods {
            let mut retrieval = advanced.clone();
            retrieval.method = *method;
            let config = RunConfig {
                profile: *profile,
                retrieval,
            };
            configurations.push(PlannedConfig {
                config_id: config_id(&config)?,
                config,
                supported: profile.supported(),
                unsupported_reason: (!profile.supported()).then(|| {
                    "SQLite is unavailable in this build; enable the sqlite feature".into()
                }),
            });
        }
    }
    Ok(ExperimentPlan {
        schema_version: 1,
        dataset: catalog.identity,
        planned_rows: cases.len() * configurations.len(),
        cases,
        configurations,
        assessment_types: vec!["deterministic".into(), "manual".into(), "ai_paused".into()],
        codex_calls: 0,
        ai_judge_calls: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retrieval::RetrievalConfig;
    use std::collections::BTreeSet;

    #[test]
    fn full_plan_has_sixteen_variants_and_accounts_for_every_question() {
        let plan = plan(&METHODS, &PROFILES, &[], &RetrievalConfig::default()).unwrap();
        assert_eq!(plan.configurations.len(), 16);
        assert_eq!(plan.cases.len(), 18);
        assert_eq!(plan.planned_rows, 288);
        assert_eq!(plan.codex_calls, 0);
        assert_eq!(plan.ai_judge_calls, 0);
        assert_eq!(
            plan.configurations
                .iter()
                .map(|row| &row.config_id)
                .collect::<BTreeSet<_>>()
                .len(),
            16
        );
        for row in &plan.configurations {
            assert_eq!(row.supported, row.config.profile.supported());
            assert_eq!(row.unsupported_reason.is_some(), !row.supported);
        }
    }

    #[test]
    fn plan_selection_and_configuration_identity_are_strict() {
        let original = RetrievalConfig::default();
        let selected = plan(
            &[SearchMethod::Bm25],
            &[StorageProfile::DocumentsSections],
            &["T02".into()],
            &original,
        )
        .unwrap();
        assert_eq!(selected.planned_rows, 1);
        assert_eq!(selected.cases[0].id, "T02");
        let first = &selected.configurations[0];
        let mut changed = first.config.clone();
        changed.retrieval.top_k = 6;
        assert_ne!(first.config_id, config_id(&changed).unwrap());
        for bad in [Vec::new(), vec![SearchMethod::Bm25, SearchMethod::Bm25]] {
            assert!(plan(&bad, &PROFILES, &[], &original).is_err());
        }
        assert!(plan(&METHODS, &PROFILES, &["NOT_A_CASE".into()], &original).is_err());
        let invalid = RetrievalConfig {
            top_k: 0,
            ..original
        };
        assert!(plan(&METHODS, &PROFILES, &[], &invalid).is_err());
    }

    #[test]
    fn floating_configuration_identity_survives_json_storage() {
        for index in 1..1000 {
            let config = RunConfig {
                profile: StorageProfile::DocumentsSections,
                retrieval: RetrievalConfig {
                    vector_weight: (index as f64 * std::f64::consts::PI).fract(),
                    ..Default::default()
                },
            };
            let restored: RunConfig =
                serde_json::from_slice(&serde_json::to_vec(&config).unwrap()).unwrap();
            assert_eq!(
                config_id(&config).unwrap(),
                config_id(&restored).unwrap(),
                "A valid configuration changed identity after persistence"
            );
        }
    }
}
