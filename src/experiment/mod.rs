pub mod arms;
pub mod budget;
pub mod dataset;
pub mod metrics;
pub mod runner;
pub mod types;

use serde_json::json;

pub async fn cli(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("extend-window") => {
            if args
                .iter()
                .filter(|a| a.as_str() == "--authorize-continuation")
                .count()
                != 1
            {
                return Err(
                    "Window continuation requires explicit product-lead authorization".into(),
                );
            }
            let values: Vec<_> = args[1..]
                .iter()
                .filter(|a| a.as_str() != "--authorize-continuation")
                .cloned()
                .collect();
            let flags = parse_flags(&values, &["--ledger", "--seconds"])?;
            let path = std::path::Path::new(flags.get("--ledger").ok_or("Missing --ledger")?);
            if !path.is_file() {
                return Err("Continuation requires an existing budget ledger".into());
            }
            let seconds = flags
                .get("--seconds")
                .ok_or("Missing --seconds")?
                .parse()
                .map_err(|_| "Invalid --seconds")?;
            let mut ledger = budget::Ledger::open(path)?;
            ledger.extend_window(seconds)?;
            println!(
                "{}",
                serde_json::to_string(&ledger.state).map_err(|_| "Cannot encode budget audit")?
            );
            Ok(())
        }
        Some("arms") if args.len() == 1 => {
            println!("{}", json!({"schema_version":1,"arms":types::Arm::ALL}));
            Ok(())
        }
        Some("run" | "grade") if !args.iter().any(|arg| arg == "--live") => {
            Err("Live experiment execution requires --live".into())
        }
        Some("validate") if args.len() == 1 => {
            let (questions, labels) = dataset::validate_bundled()?;
            println!(
                "{}",
                json!({"schema_version":1,"cases":questions.cases.len(),"criteria":labels.cases.len(),"questions_sha256":dataset::questions_hash(&questions)?})
            );
            Ok(())
        }
        Some("run") => {
            let flags = parse_flags(
                &args[1..],
                &[
                    "--output",
                    "--ledger",
                    "--arms",
                    "--cases",
                    "--max-cases",
                    "--max-generations",
                    "--max-judgements",
                    "--max-seconds",
                ],
            )?;
            let get = |name: &str| {
                flags
                    .get(name)
                    .cloned()
                    .ok_or_else(|| format!("Missing {name}"))
            };
            let number = |name: &str| {
                get(name)?
                    .parse::<usize>()
                    .map_err(|_| format!("Invalid {name}"))
            };
            let arms = get("--arms")?
                .split(',')
                .map(types::Arm::parse)
                .collect::<Result<Vec<_>, _>>()?;
            let cases = get("--cases")?.split(',').map(str::to_owned).collect();
            let limits = types::Limits {
                max_cases: number("--max-cases")?,
                max_generations: number("--max-generations")?,
                max_judgements: number("--max-judgements")?,
                max_seconds: number("--max-seconds")? as u64,
                per_call_seconds: 120,
                concurrency: 1,
                max_search_calls: 4,
                top_k: 3,
            };
            let (questions, labels) = dataset::validate_bundled()?;
            runner::run(
                runner::RunOptions {
                    output: get("--output")?.into(),
                    ledger: get("--ledger")?.into(),
                    arms,
                    cases,
                    limits,
                },
                questions,
                labels,
            )
            .await
        }
        _ => Err("Usage: experiment arms | validate | run --live | grade --live | report".into()),
    }
}

fn parse_flags(
    args: &[String],
    allowed: &[&str],
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let mut flags = std::collections::BTreeMap::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--live" {
            i += 1;
            continue;
        }
        if !allowed.contains(&args[i].as_str())
            || i + 1 >= args.len()
            || args[i + 1].starts_with("--")
        {
            return Err("Unknown flag or missing experiment flag value".into());
        }
        if flags.insert(args[i].clone(), args[i + 1].clone()).is_some() {
            return Err("Duplicate experiment flag".into());
        }
        i += 2;
    }
    Ok(flags)
}
