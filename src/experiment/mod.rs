pub mod arms;
pub mod dataset;
pub mod types;

use serde_json::json;

pub async fn cli(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("arms") if args.len() == 1 => {
            println!("{}", json!({"schema_version":1,"arms":types::Arm::ALL}));
            Ok(())
        }
        Some("run" | "grade") if !args.iter().any(|arg| arg == "--live") => {
            Err("Live experiment execution requires --live".into())
        }
        _ => Err("Usage: experiment arms | validate | run --live | grade --live | report".into()),
    }
}
