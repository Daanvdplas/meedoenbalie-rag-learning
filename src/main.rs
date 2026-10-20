use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    response::Html,
    routing::{get, post},
};
use meedoen_retrieval::{DOCUMENT, Index};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

type Shared = Arc<Mutex<Index>>;
type ApiError = (StatusCode, Json<Value>);
fn error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({"error": message.into()})))
}
#[derive(Deserialize)]
struct Search {
    question: String,
}
async fn search(
    State(index): State<Shared>,
    Json(request): Json<Search>,
) -> Result<Json<Value>, ApiError> {
    meedoen_retrieval::validate_query(&request.question)
        .map_err(|e| error(StatusCode::BAD_REQUEST, e))?;
    let result = tokio::task::spawn_blocking(move || {
        index
            .lock()
            .map_err(|_| "Model unavailable".to_string())?
            .search(&request.question)
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Search worker failed"))?
    .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!(result)))
}
#[derive(Deserialize)]
struct Review {
    question_id: String,
    verdict: String,
}
async fn review(Json(request): Json<Review>) -> Result<Json<Value>, ApiError> {
    if !["correct", "attention", "unsure"].contains(&request.verdict.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "Choose a verdict first"));
    }
    let expected: Value =
        serde_json::from_str(include_str!("../fixtures/expectations.json")).unwrap();
    let case = expected["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["question_id"] == request.question_id)
        .ok_or_else(|| {
            error(
                StatusCode::BAD_REQUEST,
                "No criteria for this custom question",
            )
        })?;
    Ok(Json(case.clone()))
}
fn app(index: Shared) -> Router {
    Router::new()
        .route(
            "/",
            get(|| async { Html(include_str!("../static/index.html")) }),
        )
        .route(
            "/api/source",
            get(|| async { Json(json!({"document":DOCUMENT,"model":meedoen_retrieval::MODEL})) }),
        )
        .route(
            "/api/questions",
            get(|| async {
                Json(
                    serde_json::from_str::<Value>(include_str!("../fixtures/questions.json"))
                        .unwrap(),
                )
            }),
        )
        .route("/api/search", post(search))
        .route("/api/review", post(review))
        .layer(DefaultBodyLimit::max(8192))
        .with_state(index)
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut index = Index::load().map_err(std::io::Error::other)?;
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("search") {
        let result = index
            .search(&args[2..].join(" "))
            .map_err(std::io::Error::other)?;
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8766").await?;
    eprintln!("Ready: http://127.0.0.1:8766 (retrieval only, no Codex)");
    axum::serve(listener, app(Arc::new(Mutex::new(index)))).await?;
    Ok(())
}
