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
    pub informed_context: bool,
    pub created_at_ms: u128,
    pub completed_at_ms: Option<u128>,
    pub parent_id: Option<String>,
    pub diagnosis: Option<String>,
}
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

const MAX_SESSIONS: usize = 256;
const MAX_RUNS: usize = 256;
const MAX_RUN_BYTES: usize = 256 * 1024;
const SESSION_TTL: Duration = Duration::from_secs(30 * 60);
struct Session {
    runs: Vec<Run>,
    seen: HashSet<String>,
    last_active: Instant,
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
    /// Called on HTTP access and admission; active runs are never expired or evicted.
    pub fn cleanup(&mut self) {
        self.sessions.retain(|_, session| {
            session.last_active.elapsed() < SESSION_TTL
                || session.runs.iter().any(|run| run.status == "pending")
        });
    }
    fn remove_oldest_completed(&mut self) -> bool {
        let oldest = self
            .sessions
            .iter()
            .flat_map(|(session_id, session)| {
                session
                    .runs
                    .iter()
                    .filter(|run| run.status != "pending")
                    .map(move |run| (session_id.clone(), run.id.clone(), run.created_at_ms))
            })
            .min_by_key(|(_, _, at)| *at);
        if let Some((session_id, id, _)) = oldest {
            self.sessions
                .get_mut(&session_id)
                .unwrap()
                .runs
                .retain(|run| run.id != id);
            true
        } else {
            false
        }
    }
    pub fn session(&mut self, seen: &[String]) -> Result<String, String> {
        if seen.len() > 5
            || seen
                .iter()
                .any(|id| !["Q1", "Q2", "Q3", "Q4", "Q5"].contains(&id.as_str()))
        {
            return Err("Ongeldige lijst met eerder bekeken vragen.".into());
        }
        let mut entropy = [0u8; 32];
        getrandom::fill(&mut entropy).map_err(|_| "Veilige sessie niet beschikbaar.")?;
        let id: String = entropy.iter().map(|byte| format!("{byte:02x}")).collect();
        self.cleanup();
        if self.sessions.len() >= MAX_SESSIONS {
            let oldest = self
                .sessions
                .iter()
                .filter(|(_, session)| !session.runs.iter().any(|run| run.status == "pending"))
                .min_by_key(|(_, session)| session.last_active)
                .map(|(id, _)| id.clone())
                .ok_or("De demo is druk. Probeer het straks opnieuw.")?;
            self.sessions.remove(&oldest);
        }
        self.sessions.insert(
            id.clone(),
            Session {
                runs: vec![],
                seen: seen.iter().cloned().collect(),
                last_active: Instant::now(),
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
        self.cleanup();
        let state = self
            .sessions
            .get(session)
            .ok_or("Testsessie bestaat niet meer; start een nieuwe sessie.")?;
        if state.runs.iter().any(|run| run.status == "pending") {
            return Err("Er draait al een vraag in deze sessie. Wacht tot die klaar is.".into());
        }
        let question_id = prepared_id(&question);
        let informed_context = parent.is_some()
            || state
                .runs
                .iter()
                .any(|run| run.question == question && run.status == "completed")
            || question_id
                .as_ref()
                .is_some_and(|id| state.seen.contains(id));
        let run = Run {
            id: format!("r-{}-{}", now(), self.sequence + 1),
            question_id,
            question,
            top_k,
            context,
            status: "pending".into(),
            result: None,
            error: None,
            first_judgement: None,
            reviews: vec![],
            criteria_revealed: false,
            informed_context,
            created_at_ms: now(),
            completed_at_ms: None,
            parent_id: parent,
            diagnosis: None,
        };
        if serde_json::to_vec(&run)
            .map_err(|_| "Uitvoering kon niet worden opgeslagen.")?
            .len()
            > MAX_RUN_BYTES / 2
        {
            return Err("Uitvoering bevat te veel gegevens.".into());
        }
        // No eviction or sequence mutation happens for a rejected request.
        if self.sessions[session].runs.len() >= 40 {
            self.sessions.get_mut(session).unwrap().runs.remove(0);
        }
        while self
            .sessions
            .values()
            .map(|session| session.runs.len())
            .sum::<usize>()
            >= MAX_RUNS
        {
            if !self.remove_oldest_completed() {
                return Err("De demo is druk. Probeer het straks opnieuw.".into());
            }
        }
        self.sequence += 1;
        let state = self.sessions.get_mut(session).unwrap();
        state.last_active = Instant::now();
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
        if serde_json::to_vec(&run).map_or(true, |bytes| bytes.len() > MAX_RUN_BYTES - 16 * 1024) {
            run.result = None;
            run.status = "failed".into();
            run.error = Some("output: Het antwoord bevat te veel gegevens.".into());
        }
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
        let informed = previous.informed_context
            || previous.first_judgement.is_some()
            || previous.criteria_revealed
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
                        if let Some(id) = hit["section"]["id"].as_str()
                            && let Some(source) = hit["section"]["source_id"].as_str()
                        {
                            actual.insert(format!("{source}:{id}"));
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
    fn rejected_requests_cannot_evict_another_visitors_history() {
        let mut store = RunStore::new();
        let mut sessions = Vec::new();
        for n in 0..MAX_SESSIONS {
            let session = store.session(&[]).unwrap();
            let run = store
                .begin(&session, &format!("Question {n}"), 3, json!({}), None)
                .unwrap();
            store.complete(&session, &run.id, Ok(json!({}))).unwrap();
            sessions.push(session);
        }
        let before: HashSet<_> = store.sessions.keys().cloned().collect();
        assert!(store.session(&["INVALID".into()]).is_err());
        assert!(store.sessions.keys().cloned().collect::<HashSet<_>>() == before);
        let before_runs: Vec<_> = sessions
            .iter()
            .map(|id| store.summary(id).unwrap()["runs"].clone())
            .collect();
        assert!(
            store
                .begin("missing-session", "Valid question", 3, json!({}), None)
                .is_err()
        );
        assert!(
            store
                .begin(
                    &sessions[0],
                    "Valid question",
                    3,
                    json!({"oversized":"x".repeat(MAX_RUN_BYTES)}),
                    None
                )
                .is_err()
        );
        let after_runs: Vec<_> = sessions
            .iter()
            .map(|id| store.summary(id).unwrap()["runs"].clone())
            .collect();
        assert_eq!(after_runs, before_runs);
    }
    #[test]
    fn expired_idle_sessions_are_removed_but_active_runs_survive_cleanup() {
        let mut store = RunStore::new();
        let idle = store.session(&[]).unwrap();
        let active = store.session(&[]).unwrap();
        let run = store
            .begin(&active, "Active question", 3, json!({}), None)
            .unwrap();
        for session in store.sessions.values_mut() {
            session.last_active = Instant::now() - SESSION_TTL - Duration::from_secs(1);
        }
        store.cleanup();
        assert!(store.summary(&idle).is_err());
        assert_eq!(store.get(&active, &run.id).unwrap().status, "pending");
        store
            .complete(&active, &run.id, Err("runtime: deadline".into()))
            .unwrap();
        store.cleanup();
        assert!(store.summary(&active).is_err());
    }
    #[test]
    fn global_history_and_result_payloads_remain_bounded() {
        let mut store = RunStore::new();
        for n in 0..300 {
            let session = store.session(&[]).unwrap();
            let run = store
                .begin(&session, &format!("Question {n}"), 3, json!({}), None)
                .unwrap();
            let result = store
                .complete(
                    &session,
                    &run.id,
                    Ok(json!({"oversized":"x".repeat(MAX_RUN_BYTES)})),
                )
                .unwrap();
            assert_eq!(result.status, "failed");
            assert!(result.result.is_none());
        }
        assert!(store.sessions.len() <= MAX_SESSIONS);
        assert!(store.sessions.values().map(|s| s.runs.len()).sum::<usize>() <= MAX_RUNS);
    }
    #[test]
    fn different_visitors_can_start_ten_independent_runs() {
        let mut store = RunStore::new();
        for n in 0..10 {
            let session = store.session(&[]).unwrap();
            assert!(
                store
                    .begin(&session, &format!("Visitor {n}"), 3, json!({}), None)
                    .is_ok(),
                "another visitor must not block admission"
            );
        }
    }
    #[test]
    fn sessions_are_unguessable_and_do_not_stop_after_sixteen_visitors() {
        let mut store = RunStore::new();
        let mut ids = HashSet::new();
        for _ in 0..32 {
            let id = store
                .session(&[])
                .expect("the seventeenth visitor must still be admitted");
            assert_eq!(id.len(), 64, "session tokens need 256 bits, not timestamps");
            assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));
            assert!(ids.insert(id));
        }
    }
    #[test]
    fn completed_history_rolls_forward_instead_of_stranding_a_visitor() {
        let mut store = RunStore::new();
        let session = store.session(&[]).unwrap();
        for n in 0..45 {
            let run = store
                .begin(&session, &format!("Question {n}"), 3, json!({}), None)
                .expect("bounded history must roll forward");
            store.complete(&session, &run.id, Ok(json!({}))).unwrap();
        }
        let summary = store.summary(&session).unwrap();
        let runs = summary["runs"].as_array().unwrap();
        assert_eq!(runs.len(), 40);
        assert_eq!(runs.last().unwrap()["question"], "Question 44");
    }
    #[test]
    fn review_does_not_count_another_documents_section_as_gardening_evidence() {
        let mut store = RunStore::new();
        let session = store.session(&[]).unwrap();
        let run = store
            .begin(
                &session,
                "Waar en wanneer is het tuinieren?",
                3,
                json!({}),
                None,
            )
            .unwrap();
        store.complete(&session,&run.id,Ok(json!({"tool_calls":[{"result":{"hits":[{"section":{"source_id":"ACT-TUIN-001","id":"S2"}},{"section":{"source_id":"ACT-FIETS-002","id":"S3"}}]}}]}))).unwrap();
        assert!(store.reveal(&session, &run.id).is_err());
        store
            .judge(&session, &run.id, Verdict::NeedsAttention)
            .unwrap();
        let revealed = store.reveal(&session, &run.id).unwrap();
        assert_eq!(
            revealed["expected"]["required_source_ids"],
            json!(["ACT-TUIN-001:S2", "ACT-TUIN-001:S3"])
        );
        let actual = revealed["actual_source_ids"].as_array().unwrap();
        assert!(actual.contains(&json!("ACT-FIETS-002:S3")));
        assert!(!actual.contains(&json!("ACT-TUIN-001:S3")));
    }
    #[test]
    fn a_pending_model_call_rejects_another_request_instead_of_queuing() {
        let mut store = RunStore::new();
        let session = store.session(&[]).unwrap();
        store.begin(&session, "First", 3, json!({}), None).unwrap();
        assert!(store.begin(&session, "Second", 3, json!({}), None).is_err());
    }
    #[test]
    fn a_revised_verdict_is_informed_even_before_criteria_reveal() {
        let (mut store, session, run) = ready();
        store
            .judge(&session, &run.id, Verdict::NeedsAttention)
            .unwrap();
        let revised = store.judge(&session, &run.id, Verdict::Correct).unwrap();
        assert!(revised.reviews[0].informed);
        assert!(!revised.criteria_revealed);
        assert_eq!(
            revised.first_judgement.unwrap().verdict,
            Verdict::NeedsAttention
        );
    }
    #[test]
    fn a_replay_or_repeat_is_informed_before_gold_is_seen() {
        let (mut store, session, original) = ready();
        let replay = store
            .begin(
                &session,
                &original.question,
                3,
                original.context.clone(),
                Some(original.id.clone()),
            )
            .unwrap();
        let replay = store.complete(&session, &replay.id, Ok(json!({}))).unwrap();
        let judged = store.judge(&session, &replay.id, Verdict::Correct).unwrap();
        assert!(judged.first_judgement.unwrap().informed);
        let repeat = store
            .begin(
                &session,
                &original.question,
                3,
                original.context.clone(),
                None,
            )
            .unwrap();
        let repeat = store.complete(&session, &repeat.id, Ok(json!({}))).unwrap();
        assert!(
            store
                .judge(&session, &repeat.id, Verdict::Correct)
                .unwrap()
                .first_judgement
                .unwrap()
                .informed
        );
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
