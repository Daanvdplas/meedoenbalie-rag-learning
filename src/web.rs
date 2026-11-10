use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use meedoen_retrieval::{
    DemoSearchMethod, Index,
    codex::{AnswerLanguage, CodexRunner},
    runs::{RunStore, Verdict},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
type App = Arc<WebState>;
type ApiError = (StatusCode, Json<Value>);
struct WebState {
    corpus: meedoen_retrieval::corpus::CorpusId,
    store: Mutex<RunStore>,
    runner: Option<CodexRunner>,
    runner_error: Option<String>,
    manifest: Value,
    sources: Value,
    configuration: Value,
}
fn error(message: impl Into<String>) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error":message.into()})),
    )
}
fn session(headers: &HeaderMap) -> Result<String, ApiError> {
    headers
        .get("x-test-session")
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.len() <= 100)
        .map(str::to_owned)
        .ok_or_else(|| error("Start eerst een testsessie."))
}
fn locked(app: &App) -> Result<std::sync::MutexGuard<'_, RunStore>, ApiError> {
    app.store
        .lock()
        .map_err(|_| error("Lokale opslag niet beschikbaar; herstart de demo."))
}
fn context(app: &App, top_k: usize, language: AnswerLanguage, method: DemoSearchMethod) -> Value {
    let mut configuration = app.configuration.clone();
    configuration["prompt_version"] = json!("web-activity-answer-only-v1");
    json!({"web_version":"activity-demo-v7","answer_language":language,"retrieval_method":method,"sources":app.manifest["sources"],"as_of":app.manifest["as_of"],"retrieval_manifest":app.manifest,"codex_configuration":configuration,"top_k":top_k})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionRequest {
    #[serde(default)]
    seen_question_ids: Vec<String>,
}
async fn new_session(
    State(app): State<App>,
    Json(input): Json<SessionRequest>,
) -> Result<Json<Value>, ApiError> {
    let mut store = locked(&app)?;
    let id = store.session(&input.seen_question_ids).map_err(error)?;
    Ok(Json(
        json!({"session_id":id,"summary":store.summary(&id).map_err(error)?}),
    ))
}
async fn summary(State(app): State<App>, headers: HeaderMap) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        locked(&app)?.summary(&session(&headers)?).map_err(error)?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RunRequest {
    question: String,
    #[serde(default)]
    top_k: Option<usize>,
    #[serde(default)]
    answer_language: AnswerLanguage,
    #[serde(default)]
    retrieval_method: DemoSearchMethod,
}
fn start(
    app: App,
    session: String,
    question: String,
    top_k: usize,
    parent: Option<String>,
    language: AnswerLanguage,
    method: DemoSearchMethod,
) -> Result<Json<Value>, ApiError> {
    let snapshot = context(&app, top_k, language, method);
    let run = locked(&app)?
        .begin(&session, &question, top_k, snapshot.clone(), parent)
        .map_err(error)?;
    let pending = json!(&run);
    let id = run.id.clone();
    tokio::spawn(async move {
        let result = if let Some(runner) = &app.runner {
            runner
                .run_for_corpus_with_method(&question, top_k, app.corpus, language, method)
                .await
                .and_then(|result| {
                    let mut result = serde_json::to_value(result).map_err(|e| e.to_string())?;
                    let actual = result["tool_calls"]
                        .as_array()
                        .and_then(|calls| calls.iter().find(|call| call["status"] == "completed"))
                        .map(|call| call["result"]["retrieval_method"].clone())
                        .ok_or("tool: Zoekmethode ontbreekt in het zoekresultaat")?;
                    if actual != snapshot["retrieval_method"] {
                        return Err("tool: Zoekmethode verschilt van de vastgelegde methode.".into());
                    }
                    result["retrieval_method"] = actual;
                    for key in ["sources", "as_of"] {
                        if result[key] != snapshot[key] {
                            return Err("tool: Broncontext verschilt van de vastgelegde bron.".into());
                        }
                    }
                    if result["manifest"] != snapshot["retrieval_manifest"] {
                        return Err("tool: Zoekindex is veranderd; deze run kan niet als dezelfde context worden opgeslagen.".into());
                    }
                    Ok(result)
                })
        } else {
            Err(app
                .runner_error
                .clone()
                .unwrap_or_else(|| "runtime: Codex is niet beschikbaar.".into()))
        };
        if let Ok(mut store) = app.store.lock() {
            let _ = store.complete(&session, &id, result);
        }
    });
    Ok(Json(pending))
}
async fn run(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<RunRequest>,
) -> Result<Json<Value>, ApiError> {
    start(
        app,
        session(&headers)?,
        input.question,
        input.top_k.unwrap_or(3),
        None,
        input.answer_language,
        input.retrieval_method,
    )
}
async fn get_run(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(
        locked(&app)?.get(&session(&headers)?, &id).map_err(error)?
    )))
}
async fn replay(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if app.runner.is_none() {
        return Err(error(
            "Opnieuw uitvoeren kan niet: Codex-runtime ontbreekt. Herstel de setup en herstart de demo.",
        ));
    }
    let session = session(&headers)?;
    let original = locked(&app)?.get(&session, &id).map_err(error)?;
    let language = serde_json::from_value(original.context["answer_language"].clone())
        .unwrap_or(AnswerLanguage::Nl);
    let method = serde_json::from_value(original.context["retrieval_method"].clone())
        .unwrap_or(DemoSearchMethod::Dense);
    let original = locked(&app)?
        .replay(
            &session,
            &id,
            &context(&app, original.top_k, language, method),
        )
        .map_err(error)?;
    start(
        app,
        session,
        original.question,
        original.top_k,
        Some(id),
        language,
        method,
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JudgementRequest {
    verdict: Verdict,
}
async fn judge(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<JudgementRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(
        locked(&app)?
            .judge(&session(&headers)?, &id, input.verdict)
            .map_err(error)?
    )))
}
async fn reveal(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        locked(&app)?
            .reveal(&session(&headers)?, &id)
            .map_err(error)?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DiagnosisRequest {
    diagnosis: String,
}
async fn diagnose(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<DiagnosisRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(
        locked(&app)?
            .diagnose(&session(&headers)?, &id, &input.diagnosis)
            .map_err(error)?
    )))
}
async fn report(State(app): State<App>, headers: HeaderMap) -> Result<impl IntoResponse, ApiError> {
    let summary = locked(&app)?.summary(&session(&headers)?).map_err(error)?;
    let mut text = format!(
        "VIJF VRAGEN — LOKAAL TESTVERSLAG\nBronnen: {} · referentiedatum {}\n\n{} van 5 vragen beoordeeld. Goed: {} · aandacht nodig: {} · onzeker: {} · met voorkennis: {}.\nEerste oordelen blijven behouden. Dit is geen automatische kwaliteitsmeting.\n\n",
        app.manifest["sources"],
        app.manifest["as_of"],
        summary["judged"],
        summary["correct"],
        summary["attention"],
        summary["unsure"],
        summary["informed"]
    );
    for run in summary["runs"].as_array().unwrap() {
        text.push_str(&format!("RUN {}\nVraag: {}\nStatus: {}\nOorspronkelijk oordeel: {}\nProbleemcategorie: {}\nBronhash: {}\nModel: {}\nConfiguratie: {}\nAntwoord: {}\nFout: {}\n\n",run["id"],run["question"],run["status"],run["first_judgement"],run["diagnosis"],run["context"]["retrieval_manifest"]["source_sha256"],run["result"]["model"],run["context"]["codex_configuration"],run["result"]["answer"]["text"],run["error"]));
    }
    Ok((
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=meedoen-testverslag.txt",
            ),
        ],
        text,
    ))
}
fn router(app: App) -> Router {
    Router::new().route("/",get(||async{Html(include_str!("../static/index.html"))}))
 .route("/demo",get(||async{Redirect::permanent("/")}))
 .route("/demo/",get(||async{Redirect::permanent("/")}))
 .route("/resultaten/",get(||async{Html(include_str!("../static/site/results.html"))}))
 .route("/techniek/",get(||async{Html(include_str!("../static/site/technical.html"))}))
 .route("/assets/site.css",get(||async{([(header::CONTENT_TYPE,"text/css; charset=utf-8")],include_str!("../static/site/site.css"))}))
 .route("/assets/language.js",get(||async{([(header::CONTENT_TYPE,"text/javascript; charset=utf-8")],include_str!("../static/site/language.js"))}))
 .route("/assets/demo.js",get(||async{([(header::CONTENT_TYPE,"text/javascript; charset=utf-8")],include_str!("../static/site/demo.js"))}))
 .route("/assets/results.js",get(||async{([(header::CONTENT_TYPE,"text/javascript; charset=utf-8")],include_str!("../static/site/results.js"))}))
 .route("/assets/results.json",get(||async{([(header::CONTENT_TYPE,"application/json; charset=utf-8")],include_str!("../static/site/results.json"))}))
 .route("/api/source",get({let app=app.clone();move||{let app=app.clone();async move{Json(json!({"sources":app.sources,"as_of":app.manifest["as_of"],"runtime_ready":app.runner.is_some(),"runtime_error":app.runner_error}))}}}))
 .route("/api/questions",get(||async{Json(serde_json::from_str::<Value>(include_str!("../fixtures/questions.json")).unwrap())}))
 .route("/api/session",post(new_session).get(summary)).route("/api/runs",post(run))
 .route("/api/runs/{id}",get(get_run)).route("/api/runs/{id}/replay",post(replay))
 .route("/api/runs/{id}/judgement",post(judge)).route("/api/runs/{id}/criteria",post(reveal))
 .route("/api/runs/{id}/diagnosis",post(diagnose)).route("/api/report",get(report))
 .layer(DefaultBodyLimit::max(8192)).with_state(app)
}
pub async fn serve(index: Index) -> Result<(), Box<dyn std::error::Error>> {
    let corpus = index.corpus();
    let (runner, runner_error, configuration) = match CodexRunner::new() {
        Ok(runner) => {
            let configuration = runner.configuration_for_corpus(corpus);
            (Some(runner), None, configuration)
        }
        Err(error) => (None, Some(error), json!({"status":"unavailable"})),
    };
    let app = Arc::new(WebState {
        corpus,
        store: Mutex::new(RunStore::new()),
        runner,
        runner_error,
        configuration,
        manifest: serde_json::to_value(index.manifest())?,
        sources: serde_json::to_value(index.sources())?,
    });
    let port = std::env::var("COA_PORT")
        .unwrap_or_else(|_| "8766".into())
        .parse::<u16>()?;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    eprintln!("Open http://127.0.0.1:{port}/ · Codex + echte bronzoekfunctie");
    axum::serve(listener, router(app)).await?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use meedoen_retrieval::AS_OF;
    #[tokio::test]
    async fn learning_site_routes_share_navigation_and_only_expose_curated_assets() {
        use tower::ServiceExt;
        let corpus = meedoen_retrieval::corpus::CorpusId::ExpandedTen;
        let app = Arc::new(WebState {
            corpus,
            store: Mutex::new(RunStore::new()),
            runner: None,
            runner_error: None,
            manifest: json!({}),
            sources: json!([]),
            configuration: json!({}),
        });
        for path in [
            "/",
            "/resultaten/",
            "/techniek/",
            "/assets/site.css",
            "/assets/results.js",
            "/assets/language.js",
            "/assets/demo.js",
            "/assets/results.json",
        ] {
            let response = router(app.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .uri(path)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "missing site route {path}"
            );
            let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
                .await
                .unwrap();
            let text = String::from_utf8(bytes.to_vec()).unwrap();
            if path.ends_with('/') {
                let navigation = text.split("</nav>").next().unwrap();
                assert!(
                    navigation.contains("href=\"/\"") && navigation.contains("/techniek/"),
                    "missing shared navigation in {path}"
                );
                assert!(
                    !navigation.contains("/resultaten/"),
                    "removed results link in navigation for {path}"
                );
            }
            if path == "/" {
                assert!(
                    text.contains("id=\"form\""),
                    "root must render the demo directly"
                );
                assert!(!text.contains("home-intro"));
            }
            assert!(!text.contains("/private/tmp/"));
            assert!(!text.contains("/Users/"));
        }
        for alias in ["/demo", "/demo/"] {
            let response = router(app.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .uri(alias)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
            assert_eq!(response.headers()[header::LOCATION], "/");
        }
        let response = router(app)
            .oneshot(
                axum::http::Request::builder()
                    .uri("/assets/PREREGISTRATION.json")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
    #[tokio::test]
    async fn expanded_source_reader_exposes_ten_sources_and_its_reference_date() {
        use tower::ServiceExt;
        let corpus = meedoen_retrieval::corpus::CorpusId::ExpandedTen;
        let app = Arc::new(WebState {
            corpus,
            store: Mutex::new(RunStore::new()),
            runner: None,
            runner_error: None,
            manifest: json!({"as_of": corpus.as_of(), "source_sha256": corpus.sha256()}),
            sources: serde_json::to_value(corpus.sources().unwrap()).unwrap(),
            configuration: json!({}),
        });
        let response = router(app)
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/source")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["as_of"], "2026-11-05");
        let sources = value["sources"].as_array().unwrap();
        assert_eq!(sources.len(), 10);
        assert_eq!(
            sources
                .iter()
                .map(|source| source["sections"].as_array().unwrap().len())
                .sum::<usize>(),
            60
        );
        assert_eq!(sources[0]["date"], "2026-10-20");
        assert_eq!(sources[9]["date"], "2026-11-05");
        assert!(!String::from_utf8_lossy(&bytes).contains("evidence_alternatives"));
    }
    #[tokio::test]
    async fn source_reader_returns_five_activity_documents_without_gold() {
        use tower::ServiceExt;
        let sources =
            meedoen_retrieval::source::parse_sources(meedoen_retrieval::DOCUMENTS).unwrap();
        let app = Arc::new(WebState {
            corpus: meedoen_retrieval::corpus::CorpusId::BaselineFive,
            store: Mutex::new(RunStore::new()),
            runner: None,
            runner_error: None,
            manifest: json!({}),
            sources: serde_json::to_value(sources).unwrap(),
            configuration: json!({}),
        });
        let response = router(app)
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/source")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["sources"].as_array().unwrap().len(), 5);
        assert_eq!(value["sources"][0]["date"], "2026-10-20");
        assert_eq!(value["sources"][1]["date"], "2026-10-18");
        assert!(!String::from_utf8_lossy(&bytes).contains("required_source_ids"));
    }
    #[tokio::test]
    async fn api_history_and_export_share_safe_metadata_for_success_failure_and_timeout() {
        use meedoen_retrieval::codex::CodexRun;
        use tower::ServiceExt;
        let app = Arc::new(WebState {
            corpus: meedoen_retrieval::corpus::CorpusId::BaselineFive,
            store: Mutex::new(RunStore::new()),
            runner: None,
            runner_error: None,
            manifest: json!({"source_sha256":"synthetic-source-hash","sources":[]}),
            sources: json!([]),
            configuration: json!({"model":"test-model","binaries":{"codex":"synthetic-fingerprint"}}),
        });
        let (session, ids) = {
            let mut store = app.store.lock().unwrap();
            let session = store.session(&[]).unwrap();
            let snapshot = context(&app, 3, AnswerLanguage::Nl, DemoSearchMethod::Dense);
            // Legacy private metadata is not part of the exported CodexRun schema.
            let result: CodexRun = serde_json::from_value(json!({
                "id":"synthetic-result", "status":"completed", "question":"Wanneer tuinieren?",
                "answer":{"text":"Dinsdag 10–12.","citations":[],"unknowns":[]},
                "tool_calls":[],"sources":[],"as_of":AS_OF,"manifest":{},
                "client_version":"codex-cli 0.163.0","model":"test-model","elapsed_ms":1,
                "runtime_path":"/Users/FAKE-PRIVATE-HOME/codex"
            }))
            .unwrap();
            let results = [
            Ok(serde_json::to_value(result).unwrap()),
            Err("runtime: Codex exited unsuccessfully; check authentication and runtime configuration".into()),
            Err("runtime: Codex run timed out".into()),
        ];
            let mut ids = Vec::new();
            for result in results {
                let run = store
                    .begin(&session, "Wanneer tuinieren?", 3, snapshot.clone(), None)
                    .unwrap();
                store.complete(&session, &run.id, result).unwrap();
                assert!(store.replay(&session, &run.id, &snapshot).is_ok());
                let mut changed = snapshot.clone();
                changed["codex_configuration"]["binaries"]["codex"] = json!("changed-fingerprint");
                assert!(store.replay(&session, &run.id, &changed).is_err());
                ids.push(run.id);
            }
            (session, ids)
        };
        let routes = [
            "/api/session".to_string(),
            format!("/api/runs/{}", ids[0]),
            format!("/api/runs/{}", ids[1]),
            format!("/api/runs/{}", ids[2]),
            "/api/report".to_string(),
        ];
        for path in routes {
            let response = router(app.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .uri(&path)
                        .header("x-test-session", &session)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
                .await
                .unwrap();
            let text = String::from_utf8(bytes.to_vec()).unwrap();
            assert!(!text.contains("FAKE-PRIVATE"), "private metadata in {path}");
            assert!(!text.contains("runtime_path"), "host path field in {path}");
            assert!(
                text.contains("synthetic-fingerprint"),
                "fingerprint missing in {path}"
            );
            assert!(
                text.contains("Wanneer tuinieren?"),
                "question missing in {path}"
            );
        }
    }

    #[tokio::test]
    async fn api_keeps_answer_language_in_context_and_replay_contract() {
        use tower::ServiceExt;
        for (requested, expected) in [
            (None, AnswerLanguage::Nl),
            (Some("nl"), AnswerLanguage::Nl),
            (Some("en"), AnswerLanguage::En),
            (Some("fr"), AnswerLanguage::Nl),
        ] {
            let app = Arc::new(WebState {
                corpus: meedoen_retrieval::corpus::CorpusId::ExpandedTen,
                store: Mutex::new(RunStore::new()),
                runner: None,
                runner_error: None,
                manifest: json!({"sources":[],"as_of":"2026-11-05"}),
                sources: json!([]),
                configuration: json!({"prompt_version":"ten-activity-grounding-v1"}),
            });
            let session = locked(&app).unwrap().session(&[]).unwrap();
            let mut body = json!({"question":"When can I join?"});
            if let Some(language) = requested {
                body["answer_language"] = json!(language);
            }
            let response = router(app.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .method("POST")
                        .uri("/api/runs")
                        .header("content-type", "application/json")
                        .header("x-test-session", &session)
                        .body(axum::body::Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            if requested == Some("fr") {
                assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
                assert!(
                    locked(&app).unwrap().summary(&session).unwrap()["runs"]
                        .as_array()
                        .unwrap()
                        .is_empty()
                );
                continue;
            }
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
                .await
                .unwrap();
            let run: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(run["context"]["answer_language"], json!(expected));
            if expected == AnswerLanguage::En {
                assert_eq!(
                    run["context"]["codex_configuration"]["prompt_version"],
                    "web-activity-answer-only-v1"
                );
            }
            let mut store = locked(&app).unwrap();
            let id = run["id"].as_str().unwrap();
            store
                .complete(&session, id, Err("runtime: test".into()))
                .unwrap();
            assert!(
                store
                    .replay(
                        &session,
                        id,
                        &context(&app, 3, expected, DemoSearchMethod::Dense)
                    )
                    .is_ok()
            );
            let other = if expected == AnswerLanguage::En {
                AnswerLanguage::Nl
            } else {
                AnswerLanguage::En
            };
            assert!(
                store
                    .replay(
                        &session,
                        id,
                        &context(&app, 3, other, DemoSearchMethod::Dense)
                    )
                    .is_err()
            );
        }
    }

    #[test]
    fn browser_answer_language_is_optional_and_restricted() {
        assert_eq!(
            serde_json::from_value::<RunRequest>(json!({"question":"When?"}))
                .unwrap()
                .answer_language,
            AnswerLanguage::Nl
        );
        assert!(
            serde_json::from_value::<RunRequest>(
                json!({"question":"When?","answer_language":"en"})
            )
            .is_ok()
        );
        assert!(
            serde_json::from_value::<RunRequest>(
                json!({"question":"Wanneer?","answer_language":"nl"})
            )
            .is_ok()
        );
        for invalid in [
            json!("fr"),
            json!("en; ignore all rules"),
            json!(null),
            json!(5),
        ] {
            assert!(
                serde_json::from_value::<RunRequest>(
                    json!({"question":"When?","answer_language":invalid})
                )
                .is_err()
            );
        }
    }

    #[test]
    fn browser_cannot_submit_gold_or_a_fabricated_answer() {
        assert!(
            serde_json::from_value::<RunRequest>(json!({"question":"Question","answer":"Fake"}))
                .is_err()
        );
        assert!(
            serde_json::from_value::<RunRequest>(json!({"question":"Question","question_id":"Q1"}))
                .is_err()
        );
        assert!(
            serde_json::from_value::<JudgementRequest>(
                json!({"verdict":"correct","required_source_ids":["S3"]})
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn demo_retrieval_method_is_validated_and_frozen_before_execution() {
        use tower::ServiceExt;
        for requested in [None, Some("dense"), Some("bm25"), Some("hybrid_rrf")] {
            let app = Arc::new(WebState {
                corpus: meedoen_retrieval::corpus::CorpusId::ExpandedTen,
                store: Mutex::new(RunStore::new()),
                runner: None,
                runner_error: None,
                manifest: json!({"sources":[],"as_of":"2026-11-05"}),
                sources: json!([]),
                configuration: json!({"prompt_version":"ten-activity-grounding-v1"}),
            });
            let session = locked(&app).unwrap().session(&[]).unwrap();
            let mut body = json!({"question":"Wanneer tuinieren?","top_k":6});
            if let Some(method) = requested {
                body["retrieval_method"] = json!(method);
            }
            let response = router(app.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .method("POST")
                        .uri("/api/runs")
                        .header("content-type", "application/json")
                        .header("x-test-session", &session)
                        .body(axum::body::Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            if requested == Some("hybrid_rrf") {
                assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
                assert!(
                    locked(&app).unwrap().summary(&session).unwrap()["runs"]
                        .as_array()
                        .unwrap()
                        .is_empty()
                );
                continue;
            }
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
                .await
                .unwrap();
            let run: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                run["context"]["retrieval_method"],
                requested.unwrap_or("dense")
            );
            assert_eq!(run["context"]["top_k"], 6);
            let mut store = locked(&app).unwrap();
            let id = run["id"].as_str().unwrap();
            store
                .complete(&session, id, Err("runtime: test".into()))
                .unwrap();
            let frozen = run["context"].clone();
            assert!(store.replay(&session, id, &frozen).is_ok());
            let mut changed = frozen;
            changed["retrieval_method"] = json!(if requested == Some("bm25") {
                "dense"
            } else {
                "bm25"
            });
            assert!(store.replay(&session, id, &changed).is_err());
        }
    }
}
