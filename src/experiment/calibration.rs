//! Small, explicitly provisional comparison against a separately reviewed reference.
use super::{
    dataset::{CaseCriteria, SectionRef},
    judge::{self, GradingLimits, Judgement, Verdict},
    types::*,
};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Worksheet {
    schema_version: String,
    worksheet_version: String,
    as_of: String,
    instruction: String,
    cases: Vec<Candidate>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Candidate {
    id: String,
    subset: String,
    question: String,
    answer: String,
    source_sections: Vec<CandidateSource>,
    human_label: Option<String>,
    human_note: Option<String>,
    clarification: String,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct CandidateSource {
    source_id: String,
    section_id: String,
    date: String,
    text: String,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceSet {
    pub schema_version: String,
    pub reference_version: String,
    pub worksheet_version: String,
    pub worksheet_sha256: String,
    pub origin: String,
    pub human_calibration: bool,
    pub reviewer: ReferenceReviewer,
    pub limitations: Vec<String>,
    pub cases: Vec<ReferenceLabel>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceReviewer {
    pub agent: String,
    pub runtime: String,
    pub model: String,
    pub reference_revision: String,
    pub reviewed_at: String,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceLabel {
    pub id: String,
    pub label: Verdict,
    pub note: String,
    pub evidence: Vec<SectionRef>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalibrationCase {
    pub id: String,
    pub subset: String,
    pub question: String,
    pub candidate_answer: String,
    pub offered_sections: Vec<crate::Section>,
    pub reference_label: Verdict,
    pub judgement: Judgement,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Agreement {
    pub subset: String,
    pub cases: usize,
    pub completed: usize,
    pub exact_agreements: usize,
    pub agreement_fraction: Option<f64>,
    pub reference_good: usize,
    pub reference_bad: usize,
    pub false_accepts: usize,
    pub false_accept_fraction: Option<f64>,
    pub false_rejects: usize,
    pub false_reject_fraction: Option<f64>,
    pub grader_abstentions: usize,
    pub reference_abstentions: usize,
    pub execution_failures: usize,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalibrationReport {
    pub schema_version: u32,
    pub execution_kind: ExecutionKind,
    pub reference: ReferenceSet,
    pub rubric_version: String,
    pub rubric_sha256: String,
    pub human_calibration: bool,
    pub provisional: bool,
    pub complete: bool,
    pub limitations: Vec<String>,
    pub cases: Vec<CalibrationCase>,
    pub agreement: Vec<Agreement>,
}

fn worksheet() -> Result<(Worksheet, String), String> {
    let bytes = include_bytes!("../../fixtures/experiments/human-review.json");
    let sheet: Worksheet =
        serde_json::from_slice(bytes).map_err(|_| "Invalid frozen review worksheet")?;
    if sheet.schema_version != "1"
        || sheet.as_of != crate::AS_OF
        || sheet.instruction.is_empty()
        || sheet.cases.len() != 8
        || sheet.cases.iter().any(|c| {
            c.human_label.is_some() || c.human_note.is_some() || c.clarification.len() > 2000
        })
    {
        return Err("Frozen worksheet drifted or contains unexpected human labels".into());
    }
    Ok((sheet, crate::manifest::hash(bytes)))
}
fn load_reference(
    path: Option<&Path>,
    sheet: &Worksheet,
    hash: &str,
) -> Result<ReferenceSet, String> {
    let bytes = if let Some(path) = path {
        let bytes = std::fs::read(path).map_err(|_| "Cannot read reference artifact")?;
        if bytes.len() > 64 * 1024 {
            return Err("Reference artifact too large".into());
        }
        bytes
    } else {
        include_bytes!("../../fixtures/experiments/ai-reference.json").to_vec()
    };
    let reference: ReferenceSet =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid reference artifact schema")?;
    let ids: std::collections::BTreeSet<_> = reference.cases.iter().map(|c| &c.id).collect();
    if reference.schema_version != "1"
        || reference.reference_version.is_empty()
        || reference.worksheet_version != sheet.worksheet_version
        || reference.worksheet_sha256 != hash
        || reference.cases.len() != 8
        || ids.len() != 8
        || !matches!(reference.origin.as_str(), "ai_agent" | "human")
        || reference.human_calibration != (reference.origin == "human")
        || sheet.cases.iter().any(|c| !ids.contains(&c.id))
    {
        return Err("Reference provenance does not match the frozen worksheet".into());
    }
    Ok(reference)
}
fn criteria(id: &str, labels: &super::dataset::LabelSet) -> Result<CaseCriteria, String> {
    let case_id=match id {"H01"|"H02"=>"P01","H03"|"H04"=>"P07","H05"|"H06"=>"P10","H07"=>"P11","H08"=>return Ok(CaseCriteria{case_id:"H08".into(),facts:vec![],conditions:vec!["No personal suitability conclusion can be drawn without knowing the activity and the person's requirements; asking necessary clarification is appropriate.".into()],forbidden_claims:vec!["Unconditionally guarantees personal suitability without the missing information.".into()],required_unknowns:vec![],relevance_labels_complete:false,relevant_sections:vec![]}),_=>return Err("Unknown calibration candidate".into())};
    labels
        .cases
        .iter()
        .find(|c| c.case_id == case_id)
        .cloned()
        .ok_or("Missing calibration rubric mapping".into())
}
fn offered(candidate: &Candidate) -> Result<Vec<crate::Section>, String> {
    let sources = crate::source::parse_sources(crate::DOCUMENTS)?;
    candidate
        .source_sections
        .iter()
        .map(|reference| {
            sources
                .iter()
                .flat_map(|s| &s.sections)
                .find(|s| {
                    s.source_id == reference.source_id
                        && s.id == reference.section_id
                        && s.source_date == reference.date
                        && s.text == reference.text
                })
                .cloned()
                .ok_or("Calibration source differs from the frozen corpus".into())
        })
        .collect()
}
pub fn summarize(cases: &[CalibrationCase], subset: &str) -> Agreement {
    let selected: Vec<_> = cases.iter().filter(|c| c.subset == subset).collect();
    let good = selected
        .iter()
        .filter(|c| c.reference_label == Verdict::Good)
        .count();
    let bad = selected
        .iter()
        .filter(|c| c.reference_label == Verdict::Bad)
        .count();
    let verdict = |c: &&CalibrationCase| c.judgement.assessment.as_ref().map(|a| a.overall.clone());
    let completed = selected
        .iter()
        .filter(|c| c.judgement.execution_status == ExecutionStatus::Completed)
        .count();
    let agreements = selected
        .iter()
        .filter(|c| verdict(c) == Some(c.reference_label.clone()))
        .count();
    let accepts = selected
        .iter()
        .filter(|c| c.reference_label == Verdict::Bad && verdict(c) == Some(Verdict::Good))
        .count();
    let rejects = selected
        .iter()
        .filter(|c| c.reference_label == Verdict::Good && verdict(c) == Some(Verdict::Bad))
        .count();
    let fraction = |n: usize, d: usize| {
        if d == 0 {
            None
        } else {
            Some(n as f64 / d as f64)
        }
    };
    Agreement {
        subset: subset.into(),
        cases: selected.len(),
        completed,
        exact_agreements: agreements,
        agreement_fraction: fraction(agreements, selected.len()),
        reference_good: good,
        reference_bad: bad,
        false_accepts: accepts,
        false_accept_fraction: fraction(accepts, bad),
        false_rejects: rejects,
        false_reject_fraction: fraction(rejects, good),
        grader_abstentions: selected
            .iter()
            .filter(|c| verdict(c) == Some(Verdict::ReviewNeeded))
            .count(),
        reference_abstentions: selected
            .iter()
            .filter(|c| c.reference_label == Verdict::ReviewNeeded)
            .count(),
        execution_failures: selected.len() - completed,
    }
}

pub async fn calibrate(
    output: &Path,
    ledger_path: &Path,
    reference_path: Option<&Path>,
    limits: GradingLimits,
) -> Result<(), String> {
    limits.validate()?;
    let (sheet, hash) = worksheet()?;
    let reference = load_reference(reference_path, &sheet, &hash)?;
    if limits.max_judgements < sheet.cases.len() {
        return Err("Calibration needs an explicit budget for all eight grading sessions".into());
    }
    let (_, labels) = super::dataset::validate_bundled()?;
    // Validate every mapping/source before runtime discovery or model requests.
    for candidate in &sheet.cases {
        criteria(&candidate.id, &labels)?;
        offered(candidate)?;
    }
    let mut ledger = super::budget::Ledger::open(ledger_path)?;
    let runtime = crate::codex::CodexRunner::new()?;
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let output_path = output;
    let output = options
        .open(output)
        .map_err(|_| "Cannot create new calibration report")?;
    drop(output);
    let started = Instant::now();
    let mut cases = vec![];
    persist(output_path, &build_report(&reference, &cases))?;
    let mut cancelled = false;
    for candidate in &sheet.cases {
        let criteria = criteria(&candidate.id, &labels)?;
        let offered = offered(candidate)?;
        // Reference labels/notes/reviewer metadata never cross this closed GradeInput boundary.
        let remaining = if cancelled {
            Duration::ZERO
        } else {
            Duration::from_secs(limits.max_seconds).saturating_sub(started.elapsed())
        };
        let answer = crate::codex::GroundedAnswer {
            text: candidate.answer.clone(),
            citations: vec![],
            unknowns: vec![],
        };
        let mut judgement = judge::assess(
            &runtime,
            &mut ledger,
            judge::GradeInput {
                question: &candidate.question,
                answer: &answer,
                offered: &offered,
                criteria: &criteria,
            },
            &limits,
            remaining,
            ExecutionKind::Live,
        )
        .await?;
        if cancelled {
            judgement.execution_status = ExecutionStatus::Cancelled;
            judgement.error = Some("cancelled".into());
        }
        cancelled |= judgement.execution_status == ExecutionStatus::Cancelled;
        let label = reference
            .cases
            .iter()
            .find(|r| r.id == candidate.id)
            .ok_or("Missing calibration reference")?;
        cases.push(CalibrationCase {
            id: candidate.id.clone(),
            subset: candidate.subset.clone(),
            question: candidate.question.clone(),
            candidate_answer: candidate.answer.clone(),
            offered_sections: offered,
            reference_label: label.label.clone(),
            judgement,
        });
        persist(output_path, &build_report(&reference, &cases))?;
        println!("{} provisional calibration complete", candidate.id);
    }
    persist(output_path, &build_report(&reference, &cases))?;
    Ok(())
}
fn build_report(reference: &ReferenceSet, cases: &[CalibrationCase]) -> CalibrationReport {
    CalibrationReport{schema_version:1,execution_kind:ExecutionKind::Live,reference:reference.clone(),rubric_version:judge::RUBRIC_VERSION.into(),rubric_sha256:crate::manifest::hash(judge::RUBRIC.as_bytes()),human_calibration:reference.human_calibration,provisional:true,complete:cases.len()==8,limitations:vec!["Only eight deliberately constructed examples: four development and four frozen validation examples.".into(),"AI-reference agreement is not human reliability. Overall-answer agreement does not validate every semantic dimension.".into(),"Agreement denominator includes all selected examples, including execution failures; false-accept/reject fractions use reference-bad/reference-good respectively.".into(),"Handcrafted candidate answers are not live generator results. Each grader session is live and separately budgeted. Partial reports are not complete calibration.".into()],cases:cases.to_vec(),agreement:vec![summarize(cases,"development"),summarize(cases,"validation")]}
}
fn persist(path: &Path, report: &CalibrationReport) -> Result<(), String> {
    use std::io::Write;
    let temporary = path.with_extension(format!("checkpoint-{}", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|_| "Cannot create calibration checkpoint")?;
    serde_json::to_writer_pretty(&mut file, report)
        .map_err(|_| "Cannot encode calibration checkpoint")?;
    file.write_all(b"\n")
        .and_then(|_| file.sync_all())
        .map_err(|_| "Cannot persist calibration checkpoint")?;
    std::fs::rename(temporary, path).map_err(|_| "Cannot finalize calibration checkpoint".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frozen_reference_and_all_source_mappings_are_valid() {
        let (sheet, hash) = worksheet().unwrap();
        let reference = load_reference(None, &sheet, &hash).unwrap();
        assert!(!reference.human_calibration);
        assert_eq!(reference.origin, "ai_agent");
        let (_, labels) = super::super::dataset::validate_bundled().unwrap();
        for candidate in &sheet.cases {
            assert!(!offered(candidate).unwrap().is_empty());
            criteria(&candidate.id, &labels).unwrap();
        }
    }
}
