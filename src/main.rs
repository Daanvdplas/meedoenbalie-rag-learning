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
        "experiment" => {
            meedoen_retrieval::experiment::cli(&args)
                .await
                .map_err(std::io::Error::other)?;
            return Ok(());
        }
        "mcp" if args.is_empty() => {
            let mut index: Option<Index> = None;
            meedoen_retrieval::mcp::serve(
                std::io::stdin().lock(),
                std::io::stdout().lock(),
                |query, top_k| {
                    if index.is_none() {
                        index = Some(Index::load()?);
                    }
                    let result = index.as_mut().unwrap().search_with_top_k(query, top_k)?;
                    serde_json::to_value(result).map_err(|error| error.to_string())
                },
            )
            .map_err(std::io::Error::other)?;
            return Ok(());
        }
        "ask" => {
            let top_k = parse_top_k(&mut args).map_err(std::io::Error::other)?;
            let runner =
                meedoen_retrieval::codex::CodexRunner::new().map_err(std::io::Error::other)?;
            let result = runner
                .run(&args.join(" "), top_k)
                .await
                .map_err(std::io::Error::other)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }
        "search" | "eval" => {
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
            let mut index = Index::load().map_err(std::io::Error::other)?;
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
    let index = Index::load().map_err(std::io::Error::other)?;
    web::serve(index).await
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
