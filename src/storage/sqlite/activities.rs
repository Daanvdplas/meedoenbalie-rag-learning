use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::retrieval::{
    ActivityRecord, Environment, FactEvidence, Schedule, Sourced, TimeWindow, Weekday,
};

pub(super) fn write(
    transaction: &Transaction<'_>,
    activity: &ActivityRecord,
) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO activities(activity_id, title, source_date) VALUES(?1, ?2, ?3)",
            params![activity.id, activity.title, activity.source_date],
        )
        .map_err(|e| e.to_string())?;
    for (position, section_id) in activity.section_ids.iter().enumerate() {
        transaction.execute(
            "INSERT INTO activity_section_links(activity_id, section_id, position) VALUES(?1, ?2, ?3)",
            params![activity.id, section_id, position as i64],
        ).map_err(|e| e.to_string())?;
    }
    if let Some(schedule) = &activity.schedule {
        transaction.execute(
            "INSERT INTO activity_schedules(activity_id, start_minutes, end_minutes) VALUES(?1, ?2, ?3)",
            params![activity.id, schedule.value.time.start_minutes, schedule.value.time.end_minutes],
        ).map_err(|e| e.to_string())?;
        for (position, weekday) in schedule.value.weekdays.iter().enumerate() {
            let name = serde_json::to_value(weekday).map_err(|e| e.to_string())?;
            transaction.execute(
                "INSERT INTO activity_weekdays(activity_id, position, weekday) VALUES(?1, ?2, ?3)",
                params![activity.id, position as i64, name.as_str().ok_or("Invalid weekday encoding")?],
            ).map_err(|e| e.to_string())?;
        }
        write_evidence(transaction, &activity.id, "schedule", &schedule.evidence)?;
    }
    if let Some(environment) = &activity.environment {
        let name = serde_json::to_value(environment.value).map_err(|e| e.to_string())?;
        transaction
            .execute(
                "INSERT INTO activity_environments(activity_id, environment) VALUES(?1, ?2)",
                params![
                    activity.id,
                    name.as_str().ok_or("Invalid environment encoding")?
                ],
            )
            .map_err(|e| e.to_string())?;
        write_evidence(
            transaction,
            &activity.id,
            "environment",
            &environment.evidence,
        )?;
    }
    Ok(())
}

fn write_evidence(
    transaction: &Transaction<'_>,
    id: &str,
    field: &str,
    evidence: &[FactEvidence],
) -> Result<(), String> {
    for (position, source) in evidence.iter().enumerate() {
        transaction.execute(
            "INSERT INTO activity_fact_evidence(activity_id, field_name, position, source_id, section_id, source_date, quote) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, field, position as i64, source.source_id, source.section_id, source.source_date, source.quote],
        ).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(super) fn read(connection: &Connection) -> Result<Vec<ActivityRecord>, String> {
    let mut statement = connection
        .prepare("SELECT activity_id, title, source_date FROM activities ORDER BY activity_id")
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    for row in rows {
        let (id, title, source_date) = row.map_err(|e| e.to_string())?;
        let section_ids = strings(
            connection,
            "SELECT section_id FROM activity_section_links WHERE activity_id = ?1 ORDER BY position",
            &id,
        )?;
        let time: Option<TimeWindow> = connection
            .query_row(
                "SELECT start_minutes, end_minutes FROM activity_schedules WHERE activity_id = ?1",
                [&id],
                |row| {
                    Ok(TimeWindow {
                        start_minutes: row.get(0)?,
                        end_minutes: row.get(1)?,
                    })
                },
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let weekday_names = strings(
            connection,
            "SELECT weekday FROM activity_weekdays WHERE activity_id = ?1 ORDER BY position",
            &id,
        )?;
        let schedule_evidence = read_evidence(connection, &id, "schedule")?;
        let schedule = if let Some(time) = time {
            let weekdays = weekday_names
                .into_iter()
                .map(|name| {
                    serde_json::from_value::<Weekday>(serde_json::Value::String(name))
                        .map_err(|e| e.to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            Some(Sourced {
                value: Schedule { weekdays, time },
                evidence: schedule_evidence,
            })
        } else {
            if !weekday_names.is_empty() || !schedule_evidence.is_empty() {
                return Err("Unknown schedule has unexpected stored facts or evidence".into());
            }
            None
        };
        let environment_name: Option<String> = connection
            .query_row(
                "SELECT environment FROM activity_environments WHERE activity_id = ?1",
                [&id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let environment_evidence = read_evidence(connection, &id, "environment")?;
        let environment = if let Some(name) = environment_name {
            Some(Sourced {
                value: serde_json::from_value::<Environment>(serde_json::Value::String(name))
                    .map_err(|e| e.to_string())?,
                evidence: environment_evidence,
            })
        } else {
            if !environment_evidence.is_empty() {
                return Err("Unknown environment has unexpected stored evidence".into());
            }
            None
        };
        result.push(ActivityRecord {
            id,
            title,
            source_date,
            section_ids,
            schedule,
            environment,
        });
    }
    Ok(result)
}

fn strings(connection: &Connection, sql: &str, id: &str) -> Result<Vec<String>, String> {
    let mut statement = connection.prepare(sql).map_err(|e| e.to_string())?;
    statement
        .query_map([id], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())
}

fn read_evidence(
    connection: &Connection,
    id: &str,
    field: &str,
) -> Result<Vec<FactEvidence>, String> {
    let mut statement = connection.prepare("SELECT source_id, section_id, source_date, quote FROM activity_fact_evidence WHERE activity_id = ?1 AND field_name = ?2 ORDER BY position").map_err(|e| e.to_string())?;
    statement
        .query_map(params![id, field], |row| {
            Ok(FactEvidence {
                source_id: row.get(0)?,
                section_id: row.get(1)?,
                source_date: row.get(2)?,
                quote: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())
}
