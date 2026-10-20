use crate::Section;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Source {
    pub id: String,
    pub date: String,
    pub sections: Vec<Section>,
}
pub fn parse_source(text: &str) -> Result<Source, String> {
    let mut id = None;
    let mut date = None;
    for line in text.lines().take_while(|line| !line.starts_with("## ")) {
        if let Some(value) = line.strip_prefix("ID: ")
            && id.replace(value.trim().to_string()).is_some()
        {
            return Err("Duplicate document ID".into());
        }
        if let Some(value) = line.strip_prefix("Brondatum: ")
            && date.replace(value.trim().to_string()).is_some()
        {
            return Err("Duplicate source date".into());
        }
    }
    let id = id
        .filter(|value| !value.is_empty())
        .ok_or("Missing document ID")?;
    let date = date.ok_or("Missing source date")?;
    validate_date(&date)?;
    Ok(Source {
        id,
        date,
        sections: crate::sections(text)?,
    })
}
pub fn validate_date(date: &str) -> Result<(), String> {
    let bytes = date.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return Err("Expected date YYYY-MM-DD".into());
    }
    let year: u32 = date[..4].parse().map_err(|_| "Invalid year")?;
    let month: usize = date[5..7].parse().map_err(|_| "Invalid month")?;
    let day: u32 = date[8..].parse().map_err(|_| "Invalid day")?;
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if year == 0 || !(1..=12).contains(&month) || day == 0 || day > days[month - 1] {
        return Err("Invalid calendar date".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const VALID: &str = "# Title\nID: DOC-A\nBrondatum: 2026-10-20\n\n## S1 — When\nEvery Tuesday.\n\nAn exact second paragraph.\n\n## S6 — Places\nTwo places on 2026-09-01; not checked since.\n";
    #[test]
    fn provenance_comes_from_source_and_keeps_paragraphs() {
        let source = parse_source(VALID).unwrap();
        assert_eq!(source.id, "DOC-A");
        assert_eq!(source.date, "2026-10-20");
        assert_eq!(
            source.sections[0].text,
            "Every Tuesday.\n\nAn exact second paragraph."
        );
        assert_eq!(source.sections[1].id, "S6");
        assert_eq!(source.sections.len(), 2);
    }
    #[test]
    fn rejects_missing_or_ambiguous_metadata_and_invalid_calendar_dates() {
        for invalid in [
            VALID.replace("ID: DOC-A\n", ""),
            VALID.replace("Brondatum: 2026-10-20\n", ""),
            VALID.replace("ID: DOC-A", "ID: DOC-A\nID: DOC-B"),
            VALID.replace("2026-10-20", "2026-02-30"),
            VALID.replace("2026-10-20", "20-10-2026"),
            VALID.replace("## S6 — Places", "## S1 — Places"),
            VALID.replace("## S1 — When", "##  — When"),
            VALID.replace("Every Tuesday.\n\nAn exact second paragraph.", ""),
        ] {
            assert!(
                parse_source(&invalid).is_err(),
                "accepted invalid source: {invalid}"
            );
        }
    }
    #[test]
    fn metadata_must_come_before_sections() {
        assert!(parse_source("# Title\n## S1 — Body\nID: DOC-A\nBrondatum: 2026-10-20").is_err());
    }
}
