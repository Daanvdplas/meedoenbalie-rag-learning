#[path = "../evals/rag/src/legacy.rs"]
mod evaluation;
mod web;
use meedoen_retrieval::Index;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let command = if args.is_empty() {
        "serve".to_string()
    } else {
        args.remove(0)
    };
    match command.as_str() {
        "lab" => {
            if args.first().map(String::as_str) == Some("agent-run") {
                meedoen_retrieval::retrieval_lab::agent_run::cli(&args)
                    .await
                    .map_err(std::io::Error::other)?;
                return Ok(());
            }
            if matches!(args.first().map(String::as_str), Some("ask" | "mcp")) {
                meedoen_retrieval::retrieval_lab::live::cli(&args)
                    .await
                    .map_err(std::io::Error::other)?;
                return Ok(());
            }
            let output =
                meedoen_retrieval::retrieval_lab::cli::cli(&args).map_err(std::io::Error::other)?;
            println!("{}", serde_json::to_string_pretty(&output)?);
            return Ok(());
        }
        "experiment" => {
            meedoen_retrieval::experiment::cli(&args)
                .await
                .map_err(std::io::Error::other)?;
            return Ok(());
        }
        "mcp" => {
            let corpus = parse_corpus(&mut args).map_err(std::io::Error::other)?;
            let method = parse_mcp_method(&mut args).map_err(std::io::Error::other)?;
            if !args.is_empty() {
                return Err(
                    "Usage: mcp [--corpus baseline-five|expanded-ten|demo-twelve] [--method dense|bm25]".into(),
                );
            }
            let mut index: Option<Index> = None;
            meedoen_retrieval::mcp::serve_with_policy(
                std::io::stdin().lock(),
                std::io::stdout().lock(),
                |query, top_k| {
                    if index.is_none() {
                        index = Some(Index::load_for_corpus(corpus)?);
                    }
                    let index = index.as_mut().unwrap();
                    match method {
                        Some(method) => index.search_for_method(query, top_k, method),
                        None => serde_json::to_value(index.search_with_top_k(query, top_k)?)
                            .map_err(|error| error.to_string()),
                    }
                },
                match method {
                    Some(method) => {
                        meedoen_retrieval::mcp::SearchPolicy::for_demo_method(corpus, method)
                    }
                    None => meedoen_retrieval::mcp::SearchPolicy::for_corpus(corpus),
                },
            )
            .map_err(std::io::Error::other)?;
            return Ok(());
        }
        "ask" => {
            let corpus = parse_corpus(&mut args).map_err(std::io::Error::other)?;
            let top_k = parse_top_k(&mut args).map_err(std::io::Error::other)?;
            let runner =
                meedoen_retrieval::codex::CodexRunner::new().map_err(std::io::Error::other)?;
            let result = runner
                .run_for_corpus(&args.join(" "), top_k, corpus)
                .await
                .map_err(std::io::Error::other)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }
        "search" | "eval" => {
            let corpus = if command == "search" {
                parse_corpus(&mut args).map_err(std::io::Error::other)?
            } else {
                meedoen_retrieval::corpus::CorpusId::BaselineFive
            };
            let top_k = parse_top_k(&mut args).map_err(std::io::Error::other)?;
            if command == "eval" && !args.is_empty() {
                return Err("Usage: eval [--top-k 1..6]".into());
            }
            let question = if command == "search" {
                Some(
                    meedoen_retrieval::validate_query(&args.join(" "))
                        .map_err(std::io::Error::other)?,
                )
            } else {
                None
            };
            let mut index = Index::load_for_corpus(corpus).map_err(std::io::Error::other)?;
            let output = if let Some(question) = question {
                json!(
                    index
                        .search_with_top_k(&question, top_k)
                        .map_err(std::io::Error::other)?
                )
            } else {
                evaluation::run(&mut index, top_k).map_err(std::io::Error::other)?
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
            return Ok(());
        }
        "serve" if args.is_empty() => {}
        _ => {
            return Err(
                "Usage: [serve | search [--top-k 1..6] QUESTION | eval [--top-k 1..6] | mcp | ask [--top-k 1..6] QUESTION]".into(),
            );
        }
    }
    let index = Index::load_for_corpus(meedoen_retrieval::corpus::CorpusId::DemoTwelve)
        .map_err(std::io::Error::other)?;
    web::serve(index).await
}

fn parse_corpus(args: &mut Vec<String>) -> Result<meedoen_retrieval::corpus::CorpusId, String> {
    if args.iter().filter(|arg| arg.as_str() == "--corpus").count() > 1 {
        return Err("Duplicate --corpus option".into());
    }
    let Some(position) = args.iter().position(|arg| arg == "--corpus") else {
        return Ok(meedoen_retrieval::corpus::CorpusId::BaselineFive);
    };
    let id = args.get(position + 1).ok_or("Missing --corpus value")?;
    let corpus = serde_json::from_value(serde_json::Value::String(id.clone()))
        .map_err(|_| "Select --corpus baseline-five, expanded-ten or demo-twelve")?;
    args.drain(position..=position + 1);
    Ok(corpus)
}

fn parse_mcp_method(
    args: &mut Vec<String>,
) -> Result<Option<meedoen_retrieval::DemoSearchMethod>, String> {
    if args.iter().filter(|arg| arg.as_str() == "--method").count() > 1 {
        return Err("Duplicate --method option".into());
    }
    let Some(position) = args.iter().position(|arg| arg == "--method") else {
        return Ok(None);
    };
    let value = args.get(position + 1).ok_or("Missing --method value")?;
    let method = serde_json::from_value(serde_json::Value::String(value.clone()))
        .map_err(|_| "Select --method dense or bm25")?;
    args.drain(position..=position + 1);
    Ok(Some(method))
}

fn parse_top_k(args: &mut Vec<String>) -> Result<usize, String> {
    if args.iter().filter(|arg| arg.as_str() == "--top-k").count() > 1 {
        return Err("Duplicate --top-k option".into());
    }
    let Some(position) = args.iter().position(|arg| arg == "--top-k") else {
        return Ok(meedoen_retrieval::DEFAULT_TOP_K);
    };
    let value = args
        .get(position + 1)
        .ok_or("Missing --top-k value")?
        .parse()
        .map_err(|_| "Expected an integer for --top-k")?;
    meedoen_retrieval::ranking::validate_top_k(value)?;
    args.drain(position..=position + 1);
    Ok(value)
}
#[cfg(test)]
mod cli_tests {
    use super::*;
    #[test]
    fn explicit_corpus_selection_preserves_question_and_legacy_default() {
        let mut args = vec!["--corpus".into(), "expanded-ten".into(), "Wanneer?".into()];
        assert_eq!(
            parse_corpus(&mut args).unwrap(),
            meedoen_retrieval::corpus::CorpusId::ExpandedTen
        );
        assert_eq!(args, ["Wanneer?"]);
        assert_eq!(
            parse_corpus(&mut args).unwrap(),
            meedoen_retrieval::corpus::CorpusId::BaselineFive
        );
        assert!(parse_corpus(&mut vec!["--corpus".into(), "other".into()]).is_err());
        assert!(parse_corpus(&mut vec!["--corpus".into()]).is_err());
        assert!(
            parse_corpus(&mut vec![
                "--corpus".into(),
                "expanded-ten".into(),
                "--corpus".into(),
                "baseline-five".into()
            ])
            .is_err()
        );
    }
    #[test]
    fn parses_bounded_k_without_losing_question_text() {
        let mut args = vec!["--top-k".into(), "2".into(), "Where?".into()];
        assert_eq!(parse_top_k(&mut args).unwrap(), 2);
        assert_eq!(args, ["Where?"]);
        for value in ["0", "7", "oops"] {
            assert!(parse_top_k(&mut vec!["--top-k".into(), value.into()]).is_err());
        }
        assert!(parse_top_k(&mut vec!["--top-k".into()]).is_err());
    }
}
