use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::Section;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Environment {
    Indoor,
    Outdoor,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TimeWindow {
    pub start_minutes: u16,
    pub end_minutes: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Schedule {
    pub weekdays: Vec<Weekday>,
    pub time: TimeWindow,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FactEvidence {
    pub source_id: String,
    pub section_id: String,
    pub source_date: String,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Sourced<T> {
    pub value: T,
    pub evidence: Vec<FactEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActivityRecord {
    pub id: String,
    pub title: String,
    pub source_date: String,
    pub section_ids: Vec<String>,
    pub schedule: Option<Sourced<Schedule>>,
    pub environment: Option<Sourced<Environment>>,
}

pub(super) fn activity_records(sections: &[Section]) -> Result<Vec<ActivityRecord>, String> {
    let mut grouped: BTreeMap<&str, Vec<&Section>> = BTreeMap::new();
    for section in sections {
        grouped.entry(&section.source_id).or_default().push(section);
    }
    grouped
        .into_iter()
        .map(|(id, sections)| {
            let first = sections[0];
            if sections.iter().any(|section| {
                section.source_date != first.source_date
                    || section.source_title != first.source_title
            }) {
                return Err("Sections of one activity disagree on source title/date".into());
            }
            let mut schedules = Vec::new();
            let mut environments = Vec::new();
            for section in &sections {
                let evidence = FactEvidence {
                    source_id: section.source_id.clone(),
                    section_id: section.id.clone(),
                    source_date: section.source_date.clone(),
                    quote: section.text.clone(),
                };
                for line in section.text.lines() {
                    if let Some(schedule) = explicit_schedule(line) {
                        schedules.push((schedule, evidence.clone()));
                    }
                }
                for sentence in section.text.split(['.', '!', '?']) {
                    if let Some(environment) = explicit_environment(sentence) {
                        environments.push((environment, evidence.clone()));
                    }
                }
            }
            Ok(ActivityRecord {
                id: id.to_owned(),
                title: first.source_title.clone(),
                source_date: first.source_date.clone(),
                section_ids: sections.iter().map(|section| section.id.clone()).collect(),
                schedule: consistent_fact(schedules),
                environment: consistent_fact(environments),
            })
        })
        .collect()
}

fn consistent_fact<T: PartialEq + Clone>(values: Vec<(T, FactEvidence)>) -> Option<Sourced<T>> {
    let first = values.first()?;
    if values.iter().any(|(value, _)| *value != first.0) {
        return None;
    }
    let value = first.0.clone();
    let mut evidence = Vec::new();
    for (_, item) in values {
        if !evidence.contains(&item) {
            evidence.push(item);
        }
    }
    Some(Sourced { value, evidence })
}

/// Deliberately limited grammar: unsupported prose stays unknown, not inferred.
fn explicit_schedule(line: &str) -> Option<Schedule> {
    let line = line.trim().to_lowercase();
    let (day, rest) = line.strip_prefix("iedere ")?.split_once(" van ")?;
    let weekday = match day {
        "maandag" => Weekday::Monday,
        "dinsdag" => Weekday::Tuesday,
        "woensdag" => Weekday::Wednesday,
        "donderdag" => Weekday::Thursday,
        "vrijdag" => Weekday::Friday,
        "zaterdag" => Weekday::Saturday,
        "zondag" => Weekday::Sunday,
        _ => return None,
    };
    let (start, end) = rest.split_once(" tot ")?;
    let end = end
        .strip_suffix(" uur.")
        .or_else(|| end.strip_suffix(" uur"))?;
    let time = TimeWindow {
        start_minutes: clock_minutes(start)?,
        end_minutes: clock_minutes(end)?,
    };
    (time.start_minutes < time.end_minutes).then_some(Schedule {
        weekdays: vec![weekday],
        time,
    })
}

fn clock_minutes(value: &str) -> Option<u16> {
    let (hours, minutes) = value.split_once(['.', ':'])?;
    if minutes.len() != 2
        || !hours
            .chars()
            .chain(minutes.chars())
            .all(|value| value.is_ascii_digit())
    {
        return None;
    }
    let hours: u16 = hours.parse().ok()?;
    let minutes: u16 = minutes.parse().ok()?;
    (hours <= 24 && minutes < 60 && (hours != 24 || minutes == 0)).then_some(hours * 60 + minutes)
}

fn explicit_environment(sentence: &str) -> Option<Environment> {
    let sentence = sentence.trim().to_lowercase();
    // A controlled statement grammar, not general Dutch interpretation: "binnen"
    // can also mean "within", and arbitrary tails can add conditions or exceptions.
    match sentence.as_str() {
        "we werken binnen"
        | "we zitten binnen"
        | "we zitten binnen aan een grote tafel"
        | "de activiteit is binnen" => Some(Environment::Indoor),
        "je werkt buiten"
        | "je werkt buiten met je handen"
        | "we werken buiten"
        | "de wandeling is buiten"
        | "de activiteit is buiten" => Some(Environment::Outdoor),
        _ => None,
    }
}
