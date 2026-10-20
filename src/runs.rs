use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Correct,
    NeedsAttention,
    NotSure,
}
#[derive(Debug, Clone, Serialize)]
pub struct Judgement {
    pub verdict: Verdict,
    pub informed: bool,
    pub at_ms: u128,
}
#[derive(Debug, Clone, Serialize)]
pub struct Run {
    pub id: String,
    pub question: String,
    pub question_id: Option<String>,
    pub top_k: usize,
    pub context: Value,
    pub status: String,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub first_judgement: Option<Judgement>,
    pub reviews: Vec<Judgement>,
    pub criteria_revealed: bool,
    pub created_at_ms: u128,
    pub completed_at_ms: Option<u128>,
    pub parent_id: Option<String>,
    pub diagnosis: Option<String>,
}
use std::collections::{HashMap, HashSet};
#[derive(Default)]
struct Session {
    runs: Vec<Run>,
    seen: HashSet<String>,
}
#[derive(Default)]
pub struct RunStore {
    sessions: HashMap<String, Session>,
    sequence: u64,
}
fn now() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
fn prepared_id(question: &str) -> Option<String> {
    let questions: Vec<Value> =
        serde_json::from_str(include_str!("../fixtures/questions.json")).ok()?;
    questions
        .iter()
        .find(|item| item["text"].as_str() == Some(question))
        .and_then(|item| item["id"].as_str())
        .map(str::to_owned)
}
impl RunStore {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn session(&mut self, seen: &[String]) -> Result<String, String> {
        if self.sessions.len() >= 16 {
            return Err("Maximum van 16 lokale sessies bereikt; herstart de demo.".into());
        }
        if seen.len() > 5
            || seen
                .iter()
                .any(|id| !["Q1", "Q2", "Q3", "Q4", "Q5"].contains(&id.as_str()))
        {
            return Err("Ongeldige lijst met eerder bekeken vragen.".into());
        }
        self.sequence += 1;
        let id = format!("s-{}-{}", now(), self.sequence);
        self.sessions.insert(
            id.clone(),
            Session {
                runs: vec![],
                seen: seen.iter().cloned().collect(),
            },
        );
        Ok(id)
    }
    pub fn begin(
        &mut self,
        session: &str,
        question: &str,
        top_k: usize,
        context: Value,
        parent: Option<String>,
    ) -> Result<Run, String> {
        let question = crate::validate_query(question)?;
        crate::ranking::validate_top_k(top_k)?;
        if self
            .sessions
            .values()
            .any(|session| session.runs.iter().any(|run| run.status == "pending"))
        {
            return Err("Er draait al een vraag. Wacht tot die uitvoering klaar is.".into());
        }
        let state = self
            .sessions
            .get_mut(session)
            .ok_or("Testsessie bestaat niet meer; start een nieuwe sessie.")?;
        if state.runs.len() >= 40 {
            return Err("Deze sessie heeft 40 runs; start een nieuwe testsessie.".into());
        }
        self.sequence += 1;
        let run = Run {
            id: format!("r-{}-{}", now(), self.sequence),
            question_id: prepared_id(&question),
            question,
            top_k,
            context,
            status: "pending".into(),
            result: None,
            error: None,
            first_judgement: None,
            reviews: vec![],
            criteria_revealed: false,
            created_at_ms: now(),
            completed_at_ms: None,
            parent_id: parent,
            diagnosis: None,
        };
        state.runs.push(run.clone());
        Ok(run)
    }
    fn mutable(&mut self, session: &str, id: &str) -> Result<&mut Run, String> {
        self.sessions
            .get_mut(session)
            .and_then(|state| state.runs.iter_mut().find(|run| run.id == id))
            .ok_or_else(|| "Run niet gevonden in deze sessie.".into())
    }
    pub fn complete(
        &mut self,
        session: &str,
        id: &str,
        result: Result<Value, String>,
    ) -> Result<Run, String> {
        let run = self.mutable(session, id)?;
        if run.status != "pending" {
            return Err("De uitvoering van deze run is al vastgelegd.".into());
        }
        match result {
            Ok(value) => {
                run.status = "completed".into();
                run.result = Some(value);
            }
            Err(error) => {
                run.status = "failed".into();
                run.error = Some(error);
            }
        }
        run.completed_at_ms = Some(now());
        Ok(run.clone())
    }
    pub fn get(&self, session: &str, id: &str) -> Result<Run, String> {
        self.sessions
            .get(session)
            .and_then(|state| state.runs.iter().find(|run| run.id == id))
            .cloned()
            .ok_or_else(|| "Run niet gevonden in deze sessie.".into())
    }
    pub fn judge(&mut self, session: &str, id: &str, verdict: Verdict) -> Result<Run, String> {
        let previous = self.get(session, id)?;
        if previous.status != "completed" {
            return Err("Beoordeel alleen een afgerond antwoord.".into());
        }
        let informed = previous.criteria_revealed
            || previous
                .question_id
                .as_ref()
                .is_some_and(|qid| self.sessions[session].seen.contains(qid));
        let judgement = Judgement {
            verdict,
            informed,
            at_ms: now(),
        };
        let run = self.mutable(session, id)?;
        if run.first_judgement.is_none() {
            run.first_judgement = Some(judgement);
        } else {
            if run.reviews.len() >= 10 {
                return Err("Maximum aantal aanvullingen voor deze run bereikt.".into());
            }
            run.reviews.push(judgement);
        }
        Ok(run.clone())
    }
    pub fn reveal(&mut self, session: &str, id: &str) -> Result<Value, String> {
        let run = self.get(session, id)?;
        if run.first_judgement.is_none() {
            return Err("Geef eerst je eigen oordeel.".into());
        }
        let question_id = run
            .question_id
            .as_ref()
            .ok_or("Voor een eigen vraag zijn geen voorbereide criteria.")?;
        let expected: Value = serde_json::from_str(include_str!("../fixtures/expectations.json"))
            .map_err(|e| e.to_string())?;
        let case = expected["cases"]
            .as_array()
            .and_then(|cases| {
                cases
                    .iter()
                    .find(|case| case["question_id"] == *question_id)
            })
            .ok_or("Criteria niet gevonden.")?
            .clone();
        let mut actual = std::collections::BTreeSet::new();
        if let Some(calls) = run
            .result
            .as_ref()
            .and_then(|value| value["tool_calls"].as_array())
        {
            for call in calls {
                if let Some(hits) = call["result"]["hits"].as_array() {
                    for hit in hits {
                        if let Some(id) = hit["section"]["id"].as_str() {
                            actual.insert(id.to_owned());
                        }
                    }
                }
            }
        }
        self.mutable(session, id)?.criteria_revealed = true;
        self.sessions
            .get_mut(session)
            .unwrap()
            .seen
            .insert(question_id.clone());
        Ok(
            json!({"expected":case,"actual_source_ids":actual,"first_judgement":run.first_judgement}),
        )
    }
    pub fn replay(&self, session: &str, id: &str, context: &Value) -> Result<Run, String> {
        let run = self.get(session, id)?;
        if run.status == "pending" {
            return Err("Deze run is nog bezig.".into());
        }
        if &run.context != context {
            return Err(
                "Opnieuw uitvoeren geweigerd: bron, model of configuratie is gewijzigd.".into(),
            );
        }
        Ok(run)
    }
    pub fn diagnose(&mut self, session: &str, id: &str, diagnosis: &str) -> Result<Run, String> {
        if !["retrieval", "answer", "both", "unclear"].contains(&diagnosis) {
            return Err("Ongeldige probleemcategorie.".into());
        }
        let run = self.mutable(session, id)?;
        if run
            .reviews
            .last()
            .or(run.first_judgement.as_ref())
            .is_none_or(|judgement| judgement.verdict != Verdict::NeedsAttention)
        {
            return Err("Kies eerst 'Aandacht nodig'.".into());
        }
        run.diagnosis = Some(diagnosis.into());
        Ok(run.clone())
    }
    pub fn summary(&self, session: &str) -> Result<Value, String> {
        let state = self
            .sessions
            .get(session)
            .ok_or("Testsessie bestaat niet meer.")?;
        let mut first: HashMap<&str, &Judgement> = HashMap::new();
        for run in &state.runs {
            if let (Some(id), Some(judgement)) = (&run.question_id, &run.first_judgement) {
                first.entry(id).or_insert(judgement);
            }
        }
        Ok(
            json!({"judged":first.len(),"total":5,"correct":first.values().filter(|j|j.verdict==Verdict::Correct).count(),"attention":first.values().filter(|j|j.verdict==Verdict::NeedsAttention).count(),"unsure":first.values().filter(|j|j.verdict==Verdict::NotSure).count(),"informed":first.values().filter(|j|j.informed).count(),"runs":state.runs,"known_question_ids":state.seen}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ready() -> (RunStore, String, Run) {
        let mut store = RunStore::new();
        let session = store.session(&[]).unwrap();
        let run = store
            .begin(
                &session,
                "Waar en wanneer is het tuinieren?",
                3,
                json!({"source":"hash-a","model":"fixed"}),
                None,
            )
            .unwrap();
        let run = store
            .complete(
                &session,
                &run.id,
                Ok(json!({"answer":{"text":"Tuesday"},"tool_calls":[]})),
            )
            .unwrap();
        (store, session, run)
    }
    #[test]
    fn a_pending_model_call_rejects_another_request_instead_of_queuing() {
        let mut store = RunStore::new();
        let session = store.session(&[]).unwrap();
        store.begin(&session, "First", 3, json!({}), None).unwrap();
        assert!(store.begin(&session, "Second", 3, json!({}), None).is_err());
    }
    #[test]
    fn disclosure_requires_server_verdict_and_preserves_first_judgement() {
        let (mut store, session, run) = ready();
        assert!(store.reveal(&session, &run.id).is_err());
        store
            .judge(&session, &run.id, Verdict::NeedsAttention)
            .unwrap();
        let revealed = store.reveal(&session, &run.id).unwrap();
        assert_eq!(revealed["expected"]["question_id"], "Q1");
        let revised = store.judge(&session, &run.id, Verdict::Correct).unwrap();
        assert_eq!(
            revised.first_judgement.unwrap().verdict,
            Verdict::NeedsAttention
        );
        assert!(revised.reviews[0].informed);
    }
    #[test]
    fn custom_query_never_borrows_prepared_gold() {
        let (mut store, session, _) = ready();
        let run = store
            .begin(
                &session,
                "Waar en wanneer is het tuinieren? Edited",
                3,
                json!({}),
                None,
            )
            .unwrap();
        assert!(run.question_id.is_none());
        let run = store.complete(&session, &run.id, Ok(json!({}))).unwrap();
        store.judge(&session, &run.id, Verdict::NotSure).unwrap();
        assert!(store.reveal(&session, &run.id).is_err());
    }
    #[test]
    fn replay_refuses_changed_context_and_sessions_are_separate() {
        let (mut store, session, run) = ready();
        assert!(
            store
                .replay(
                    &session,
                    &run.id,
                    &json!({"source":"changed","model":"fixed"})
                )
                .is_err()
        );
        assert_eq!(
            store
                .replay(&session, &run.id, &run.context)
                .unwrap()
                .question,
            run.question
        );
        let other = store.session(&[]).unwrap();
        assert!(store.get(&other, &run.id).is_err());
    }
    #[test]
    fn model_failure_is_recorded_without_fabricated_answer() {
        let (mut store, session, _) = ready();
        let run = store
            .begin(&session, "Vrije vraag", 3, json!({}), None)
            .unwrap();
        let failed = store
            .complete(&session, &run.id, Err("tool: unavailable".into()))
            .unwrap();
        assert_eq!(failed.status, "failed");
        assert!(failed.result.is_none());
        assert!(store.judge(&session, &run.id, Verdict::Correct).is_err());
        assert!(store.begin(&session, " ", 3, json!({}), None).is_err());
        assert!(
            store
                .begin(&session, &"x".repeat(1001), 3, json!({}), None)
                .is_err()
        );
    }
}
