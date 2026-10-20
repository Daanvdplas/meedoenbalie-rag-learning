use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use meedoen_retrieval::{
    AS_OF, Index,
    codex::CodexRunner,
    runs::{RunStore, Verdict},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
type App = Arc<WebState>;
type ApiError = (StatusCode, Json<Value>);
struct WebState {
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
fn context(app: &App, top_k: usize) -> Value {
    json!({"web_version":"human-review-v2","sources":app.manifest["sources"],"as_of":AS_OF,"retrieval_manifest":app.manifest,"codex_configuration":app.configuration,"top_k":top_k})
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
}
fn start(
    app: App,
    session: String,
    question: String,
    top_k: usize,
    parent: Option<String>,
) -> Result<Json<Value>, ApiError> {
    let snapshot = context(&app, top_k);
    let run = locked(&app)?
        .begin(&session, &question, top_k, snapshot.clone(), parent)
        .map_err(error)?;
    let pending = json!(&run);
    let id = run.id.clone();
    tokio::spawn(async move {
        let result = if let Some(runner) = &app.runner {
            runner.run(&question,top_k).await.and_then(|result|{
    let result=serde_json::to_value(result).map_err(|e|e.to_string())?;
    for key in ["sources","as_of"] {if result[key]!=snapshot[key]{return Err("tool: Broncontext verschilt van de vastgelegde bron.".into());}}
    if result["manifest"]!=snapshot["retrieval_manifest"]{return Err("tool: Zoekindex is veranderd; deze run kan niet als dezelfde context worden opgeslagen.".into());}
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
    let original = locked(&app)?
        .replay(&session, &id, &context(&app, original.top_k))
        .map_err(error)?;
    start(app, session, original.question, original.top_k, Some(id))
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
        AS_OF,
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
 .route("/api/source",get({let app=app.clone();move||{let app=app.clone();async move{Json(json!({"sources":app.sources,"as_of":AS_OF,"runtime_ready":app.runner.is_some(),"runtime_error":app.runner_error}))}}}))
 .route("/api/questions",get(||async{Json(serde_json::from_str::<Value>(include_str!("../fixtures/questions.json")).unwrap())}))
 .route("/api/session",post(new_session).get(summary)).route("/api/runs",post(run))
 .route("/api/runs/{id}",get(get_run)).route("/api/runs/{id}/replay",post(replay))
 .route("/api/runs/{id}/judgement",post(judge)).route("/api/runs/{id}/criteria",post(reveal))
 .route("/api/runs/{id}/diagnosis",post(diagnose)).route("/api/report",get(report))
 .layer(DefaultBodyLimit::max(8192)).with_state(app)
}
pub async fn serve(index: Index) -> Result<(), Box<dyn std::error::Error>> {
    let (runner, runner_error, configuration) = match CodexRunner::new() {
        Ok(runner) => {
            let configuration = runner.configuration();
            (Some(runner), None, configuration)
        }
        Err(error) => (None, Some(error), json!({"status":"unavailable"})),
    };
    let app = Arc::new(WebState {
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
    #[tokio::test]
    async fn source_reader_returns_two_distinct_document_dates_without_gold() {
        use tower::ServiceExt;
        let sources =
            meedoen_retrieval::source::parse_sources(meedoen_retrieval::DOCUMENTS).unwrap();
        let app = Arc::new(WebState {
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
        assert_eq!(value["sources"].as_array().unwrap().len(), 2);
        assert_eq!(value["sources"][0]["date"], "2026-10-20");
        assert_eq!(value["sources"][1]["date"], "2026-10-18");
        assert!(!String::from_utf8_lossy(&bytes).contains("required_source_ids"));
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
}
