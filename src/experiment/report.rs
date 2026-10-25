use super::{
    dataset::{self, CaseCriteria, LabelSet, QuestionSet, SectionRef},
    metrics::{self, MetricsReport},
    types::{Arm, ExecutionKind, ExecutionStatus, ExperimentRecord},
};
use std::{
    collections::BTreeSet,
    fmt::Write as _,
    io::Write as _,
    path::{Path, PathBuf},
};

pub fn write_report(inputs: &[PathBuf], output: &Path) -> Result<usize, String> {
    if inputs.is_empty() || inputs.len() > 8 {
        return Err("Kies één tot acht JSONL-bestanden voor het rapport.".into());
    }
    let existing_output = std::fs::canonicalize(output).ok();
    let mut records = Vec::new();
    let mut ids = BTreeSet::new();
    for input in inputs {
        if existing_output.is_some() && std::fs::canonicalize(input).ok() == existing_output {
            return Err("Het rapport mag een bronbestand niet overschrijven.".into());
        }
        for record in super::runner::read_records(input).map_err(|_| "Het invoerbestand is leeg, ontbreekt of bevat een ongeldige uitvoering. Er is geen nieuw rapport gemaakt.")? {
            if !ids.insert(record.run_id.clone()) || records.len() >= 400 { return Err("Dubbele uitvoeringen of meer dan 400 resultaten; voeg elk artifact maar één keer toe.".into()); }
            records.push(record);
        }
    }
    let html = render(&records)?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temporary = parent.join(format!(".evaluation-report-{}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|_| "Kan het rapport niet schrijven. Kies een bestaande map.")?;
    let result = (|| {
        file.write_all(html.as_bytes())
            .map_err(|_| "Kan het rapport niet volledig schrijven.")?;
        std::fs::rename(&temporary, output).map_err(|_| "Kan het rapport niet opslaan.")
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result?;
    Ok(records.len())
}

fn escape(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&#39;".into(),
            _ => c.to_string(),
        })
        .collect()
}
fn arm_name(arm: Arm) -> &'static str {
    match arm {
        Arm::AgenticDense => "Codex zoekt verder",
        Arm::DenseOnce => "Betekeniszoeken (MiniLM)",
        Arm::Bm25 => "Woordzoeken (BM25)",
        Arm::FullContext => "Alle documenten",
    }
}
fn split_name(split: &str) -> &'static str {
    match split {
        "held_out" => "Vooraf apart gehouden",
        "development" => "Ontwikkeling",
        _ => "Pilot",
    }
}
fn type_name(kind: &str) -> &'static str {
    match kind {
        "multiple_facts" => "Meerdere feiten",
        "combined_conditions" => "Voorwaarden combineren",
        "stale_information" => "Verouderde informatie",
        "missing_information" => "Onbekende gegevens",
        "all_matches" => "Alle mogelijkheden",
        "no_matching_activity" => "Geen passend aanbod",
        _ => "Anders geformuleerd",
    }
}
fn execution_name(status: &ExecutionStatus) -> &'static str {
    match status {
        ExecutionStatus::Completed => "Voltooid",
        ExecutionStatus::TimedOut => "Time-out",
        ExecutionStatus::Cancelled => "Geannuleerd",
        ExecutionStatus::Skipped => "Niet uitgevoerd",
        ExecutionStatus::Failed => "Uitvoering mislukt",
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Good,
    Bad,
    Review,
    Failed,
    NotRun,
}
impl Outcome {
    fn key(self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Bad => "bad",
            Self::Review => "review",
            Self::Failed => "failed",
            Self::NotRun => "not_run",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Good => "Goed · AI",
            Self::Bad => "Fout",
            Self::Review => "Beoordeling nodig",
            Self::Failed => "Uitvoering mislukt",
            Self::NotRun => "Niet uitgevoerd",
        }
    }
}
fn outcome(record: &ExperimentRecord, measured: &MetricsReport) -> Outcome {
    if record.execution_status == ExecutionStatus::Skipped {
        return Outcome::NotRun;
    }
    if record.execution_status != ExecutionStatus::Completed {
        return Outcome::Failed;
    }
    if record.execution_kind == ExecutionKind::Fixture {
        return Outcome::Review;
    }
    if !measured.hard_checks_passed {
        return Outcome::Bad;
    }
    let Some(judgement) = &record.judgement else {
        return Outcome::Review;
    };
    if judgement["execution_status"] != "completed" || judgement["execution_kind"] != "live" {
        return Outcome::Review;
    }
    match judgement["assessment"]["overall"].as_str() {
        Some("good") => Outcome::Good,
        Some("bad") => Outcome::Bad,
        _ => Outcome::Review,
    }
}

struct Prepared<'a> {
    record: &'a ExperimentRecord,
    criteria: &'a CaseCriteria,
    measured: MetricsReport,
    outcome: Outcome,
}
fn catalog() -> Result<Vec<(QuestionSet, LabelSet)>, String> {
    Ok(vec![
        dataset::validate_bundled()?,
        dataset::parse(
            include_str!("../../fixtures/experiments/heldout-questions.json"),
            include_str!("../../fixtures/experiments/heldout-expectations.json"),
        )?,
    ])
}
fn bind<'a>(
    record: &ExperimentRecord,
    datasets: &'a [(QuestionSet, LabelSet)],
) -> Result<&'a CaseCriteria, String> {
    let (questions, labels) = datasets
        .iter()
        .find(|(q, _)| q.dataset_version == record.manifest.questions_version)
        .ok_or("Onbekende datasetversie; gebruik de bijbehorende rapportversie.")?;
    let case = questions
        .cases
        .iter()
        .find(|q| q.id == record.case_id)
        .ok_or("De uitvoering hoort niet bij deze vragenset.")?;
    if dataset::questions_hash(questions)? != record.manifest.questions_sha256
        || labels.dataset_version != record.manifest.expectations_version
        || crate::manifest::hash(
            &serde_json::to_vec(labels).map_err(|_| "Kan criteria niet controleren")?,
        ) != record.manifest.expectations_sha256
        || case.question != record.question
        || case.family != record.family
        || case.split != record.split
        || case.kind != record.kind
    {
        return Err("Vragen, criteria of versies wijken af van de opgeslagen uitvoering.".into());
    }
    labels
        .cases
        .iter()
        .find(|c| c.case_id == record.case_id)
        .ok_or("Verwachte criteria ontbreken.".into())
}

fn metric_value(report: &MetricsReport, id: &str) -> String {
    let metric = report
        .metrics
        .iter()
        .find(|m| m.id == id)
        .expect("metrics schema has fixed ids");
    match metric.value {
        Some(value) => match (metric.numerator, metric.denominator) {
            (Some(n), Some(d)) => {
                format!("{n} van {d} ({})", format!("{value:.2}").replace('.', ","))
            }
            _ if id == "elapsed_ms" => {
                format!("{} s", format!("{:.2}", value / 1000.0).replace('.', ","))
            }
            _ => format!("{value:.0}"),
        },
        None if ["search_calls", "mcp_calls", "input_tokens", "output_tokens"].contains(&id) => {
            "Onbekend".into()
        }
        None => "N.v.t.".into(),
    }
}

fn metric_label(id: &str) -> &'static str {
    match id {
        "first_hop_fact_coverage" => "Benodigd bewijs na de eerste zoekpoging",
        "all_hop_fact_coverage" => "Benodigd bewijs na alle zoekpogingen",
        "first_hop_precision" => "Aandeel relevante passages bij de eerste zoekpoging",
        "all_hop_precision" => "Aandeel relevante passages over alle zoekpogingen",
        "first_hop_section_recall" => "Relevante passages gevonden bij de eerste zoekpoging",
        "all_hop_section_recall" => "Relevante passages gevonden over alle zoekpogingen",
        "first_relevant_reciprocal_rank" => "Positie van het eerste relevante resultaat",
        "offered_fact_coverage" => "Benodigd bewijs in de aangeboden context",
        "search_calls" => "Zoekacties",
        "mcp_calls" => "Echte MCP-calls",
        "elapsed_ms" => "Tijd per vraag",
        "input_tokens" => "Inputtokens",
        _ => "Outputtokens",
    }
}

fn metric_explanation(id: &str) -> &'static str {
    match id {
        "first_hop_fact_coverage" | "all_hop_fact_coverage" | "offered_fact_coverage" => {
            "Aantal benodigde feiten met een complete geldige bewijscombinatie, gedeeld door alle feiten met een bronbewijsdoel. Een volledig alternatief is genoeg; dubbele passages tellen niet extra."
        }
        "first_hop_precision" | "all_hop_precision" => {
            "Aantal relevante, geldige passages gedeeld door alle unieke gevonden passages. Alleen gedefinieerd met complete relevantielabels en minstens één gevonden passage."
        }
        "first_hop_section_recall" | "all_hop_section_recall" => {
            "Aantal relevante gevonden passages gedeeld door alle vooraf relevant gelabelde passages, inclusief nuttig uitsluitingsbewijs. Bestaand bewijs niet gevonden geeft nul; geen relevant bewijsdoel geeft N.v.t."
        }
        "first_relevant_reciprocal_rank" => {
            "Eén gedeeld door de oorspronkelijke positie van het eerste relevante resultaat bij de eerste zoekpoging. Positie 2 geeft 0,50. Geen relevant resultaat geeft nul als er wel relevant bewijs bestaat."
        }
        "search_calls" => {
            "Werkelijke zoekpogingen, inclusief mislukte MCP-calls. Bij een onvolledige agenttrace is het totale aantal onbekend. Eén zoekactie is geen algemene slaagvoorwaarde."
        }
        "mcp_calls" => {
            "Werkelijke MCP-calls van Codex. Lokale selectie vooraf is geen MCP-call. Een onvolledige agenttrace geeft onbekend, geen nul."
        }
        "elapsed_ms" => {
            "Voorbereiding per vraag, lokale retrieval en generatie. Gedeelde indexopbouw valt erbuiten; MCP-opstart binnen de agentsessie telt wel mee. Dit is geen uniforme cold-startbenchmark."
        }
        _ => {
            "Door de provider gemelde tokens. Niet beschikbaar gebruik blijft onbekend en wordt niet als nul ingevuld."
        }
    }
}

fn render(records: &[ExperimentRecord]) -> Result<String, String> {
    if records.is_empty() {
        return Err("Er zijn nog geen opgeslagen uitvoeringen om te tonen.".into());
    }
    let datasets = catalog()?;
    let mut prepared = Vec::new();
    let mut ids = BTreeSet::new();
    for record in records {
        super::runner::validate_record(record)?;
        if !ids.insert(&record.run_id) {
            return Err("Dezelfde uitvoering komt meer dan één keer voor.".into());
        }
        let criteria = bind(record, &datasets)?;
        let measured = metrics::evaluate(record, criteria)?;
        metrics::validate_report(&measured)?;
        if record.metrics.as_ref().is_some_and(|saved| {
            serde_json::to_value(&measured).is_ok_and(|actual| saved != &actual)
        }) {
            return Err("Opgeslagen meetwaarden wijken af van de opnieuw berekende trace.".into());
        }
        if record
            .judgement
            .as_ref()
            .is_some_and(|j| j["rubric_criteria"] != serde_json::to_value(criteria).unwrap())
        {
            return Err("De AI-beoordeling gebruikt andere criteria dan deze dataset.".into());
        }
        let outcome = outcome(record, &measured);
        prepared.push(Prepared {
            record,
            criteria,
            measured,
            outcome,
        });
    }
    let mut body = String::new();
    let examples = records
        .iter()
        .filter(|r| r.execution_kind == ExecutionKind::Fixture)
        .count();
    write!(body, "<header><p class=eyebrow>Activiteiten / Evaluatie</p><h1>Wat laten de tests zien?</h1><p class=lead>Vergelijk het zoeken, het antwoord en de bronnen.</p><p class=calibration><strong>AI-oordelen zijn voorlopig.</strong> Geen menselijke ijking.</p></header>").unwrap();
    if examples > 0 {
        write!(body, "<aside class=example><strong>Voorbeeldresultaten · {examples} uitvoeringen</strong><p>Handgemaakte voorbeelden om het rapport te bekijken. Geen gemeten modelkwaliteit.</p></aside>").unwrap();
    }
    let questions: BTreeSet<_> = records
        .iter()
        .map(|r| (&r.manifest.questions_version, &r.case_id))
        .collect();
    write!(body, "<p class=report-count>{} opgeslagen uitvoeringen · {} vragen · {} geregistreerd als echt</p><section aria-labelledby=compare-title><h2 id=compare-title>De vier aanpakken</h2>", records.len(), questions.len(), records.len()-examples).unwrap();
    for kind in [ExecutionKind::Live, ExecutionKind::Fixture] {
        for split in ["held_out", "pilot", "development"] {
            let rows: Vec<_> = prepared
                .iter()
                .filter(|p| p.record.execution_kind == kind && p.record.split == split)
                .collect();
            if rows.is_empty() {
                continue;
            }
            write!(body, "<h3>{} · {}</h3><p class=scroll-hint>Schuif de tabel opzij voor alle uitkomsten →</p><div class=table-scroll tabindex=0 role=region aria-label=\"Vergelijking; schuif opzij voor alle uitkomsten\"><table><caption class=sr-only>Aantallen per aanpak, {} {}</caption><thead><tr><th scope=col>Aanpak</th>", if kind == ExecutionKind::Fixture { "Voorbeelden" } else { "Echte uitvoeringen" }, split_name(split), if kind == ExecutionKind::Fixture { "voorbeelden" } else { "echt" }, split_name(split)).unwrap();
            for label in [
                "Goed · AI",
                "Fout",
                "Oordeel nodig",
                "Mislukt",
                "Niet uitgevoerd",
                "Totaal",
            ] {
                write!(body, "<th scope=col>{label}</th>").unwrap();
            }
            body.push_str("</tr></thead><tbody>");
            for arm in Arm::ALL {
                let selected: Vec<_> = rows.iter().filter(|r| r.record.arm == arm).collect();
                write!(body, "<tr><th scope=row>{}</th>", arm_name(arm)).unwrap();
                if selected.is_empty() {
                    body.push_str("<td colspan=6 class=help>Geen uitvoering opgeslagen</td></tr>");
                    continue;
                }
                for outcome in [
                    Outcome::Good,
                    Outcome::Bad,
                    Outcome::Review,
                    Outcome::Failed,
                    Outcome::NotRun,
                ] {
                    write!(
                        body,
                        "<td class={}>{}</td>",
                        outcome.key(),
                        selected.iter().filter(|p| p.outcome == outcome).count()
                    )
                    .unwrap();
                }
                write!(body, "<td class=total>{}</td></tr>", selected.len()).unwrap();
            }
            body.push_str("</tbody></table></div>");
        }
    }
    body.push_str("<p class=help>Alleen opgeslagen uitvoeringen tellen mee. Voorbeelden en vraaggroepen blijven apart; vergelijk dezelfde vragen.</p><p class=help>Bewijsdekking telt complete bewijsdoelen: bij T02 kan een gevonden tijd al nuttig zijn, terwijl ontbrekende informatie over binnen werken en het adres de dekking op 0/2 houdt.</p></section>");
    render_paired(&mut body, &prepared, &datasets[1].0);
    body.push_str("<section aria-labelledby=questions-title><h2 id=questions-title>Bekijk een vraag</h2><p class=help>Volledige historie van de aangeleverde uitvoeringen, inclusief mislukte pogingen en herhalingen.</p><form id=filters aria-label=\"Filter de uitvoeringen\"><label>Aanpak<select id=arm><option value=\"\">Alle aanpakken</option>");
    for arm in Arm::ALL {
        write!(
            body,
            "<option value={}>{}</option>",
            arm.name(),
            arm_name(arm)
        )
        .unwrap();
    }
    body.push_str("</select></label><label>Vraagtype<select id=kind><option value=\"\">Alle vraagtypen</option>");
    let kinds: BTreeSet<_> = records.iter().map(|r| r.kind.as_str()).collect();
    for kind in kinds {
        write!(
            body,
            "<option value=\"{}\">{}</option>",
            escape(kind),
            type_name(kind)
        )
        .unwrap();
    }
    body.push_str("</select></label><label>Uitkomst<select id=outcome><option value=\"\">Alle uitkomsten</option>");
    for outcome in [
        Outcome::Good,
        Outcome::Bad,
        Outcome::Review,
        Outcome::Failed,
        Outcome::NotRun,
    ] {
        write!(
            body,
            "<option value={}>{}</option>",
            outcome.key(),
            outcome.label()
        )
        .unwrap();
    }
    body.push_str("</select></label><label>Vraaggroep<select id=split><option value=\"\">Alle groepen</option><option value=development>Ontwikkeling</option><option value=pilot>Pilot</option><option value=held_out>Vooraf apart gehouden</option></select></label><button type=reset>Wis filters</button></form><p id=filter-count role=status aria-live=polite></p><p id=no-results hidden>Geen uitvoeringen met deze filters. Kies een andere combinatie of wis de filters.</p><div id=questions>");
    for (index, item) in prepared.iter().enumerate() {
        render_question(&mut body, item, index);
    }
    body.push_str("</div></section><details class=method><summary>Hoe lees je deze resultaten?</summary><p>Bewijsdekking telt benodigde feiten met een geldige broncombinatie. Een float zoals 0,50 kan één van twee feiten betekenen; het is geen kans dat een antwoord klopt. Geen passend bronbewijs geeft N.v.t., geen nul.</p><p>Een AI-oordeel gaat over het antwoord. De harde controles gaan over bronidentiteit, citaten en uitvoering. Goed vereist beide en blijft voorlopig zonder menselijke ijking. Voorbeelden krijgen geen positief live kwaliteitsoordeel.</p><p>De zes apart gehouden vragen zijn nieuwe combinaties van bekende bronfeiten. Dit is een kleine verkennende vergelijking, geen algemene betrouwbaarheidstest.</p><p>Tijd omvat voorbereiding per vraag, lokale retrieval en generatie. Gedeelde indexopbouw valt erbuiten; opstart van MCP binnen de agentsessie telt wel mee. Geen uniforme cold-startbenchmark. Modelsessies zijn geen telling van verborgen providerrequests.</p></details><footer>Zelfstandig lokaal rapport · openen, filteren en uitklappen doet geen model- of netwerkverzoeken.</footer>");
    Ok(include_str!("../../static/evaluation-report.html").replace("<!-- REPORT -->", &body))
}

fn paired_delta(agent: &Prepared<'_>, baseline: &Prepared<'_>) -> Option<f64> {
    let a = agent.record;
    let b = baseline.record;
    if a.execution_kind != ExecutionKind::Live
        || b.execution_kind != ExecutionKind::Live
        || a.case_id != b.case_id
        || a.family != b.family
        || a.manifest.questions_sha256 != b.manifest.questions_sha256
        || a.manifest.expectations_sha256 != b.manifest.expectations_sha256
        || a.manifest.generator_route != b.manifest.generator_route
        || a.execution_status != ExecutionStatus::Completed
        || b.execution_status != ExecutionStatus::Completed
        || !agent.measured.hard_checks_passed
        || !baseline.measured.hard_checks_passed
        || a.manifest.model != b.manifest.model
        || a.manifest.runtime_sha256 != b.manifest.runtime_sha256
        || a.manifest.binary_sha256 != b.manifest.binary_sha256
        || a.manifest.code_mode_host_sha256 != b.manifest.code_mode_host_sha256
        || a.manifest.client_version != b.manifest.client_version
        || a.manifest.limits.top_k != b.manifest.limits.top_k
        || a.manifest.limits.max_search_calls != b.manifest.limits.max_search_calls
        || a.manifest.limits.per_call_seconds != b.manifest.limits.per_call_seconds
    {
        return None;
    }
    if b.arm == Arm::DenseOnce
        && (a.manifest.embedding_revision != b.manifest.embedding_revision
            || serde_json::to_value(&a.manifest.embedding_files).ok()
                != serde_json::to_value(&b.manifest.embedding_files).ok())
    {
        return None;
    }
    let value = |p: &Prepared<'_>| {
        p.measured
            .metrics
            .iter()
            .find(|m| m.id == "all_hop_fact_coverage")
            .and_then(|m| m.value)
    };
    Some(value(baseline)? - value(agent)?)
}

fn render_paired(body: &mut String, prepared: &[Prepared<'_>], questions: &QuestionSet) {
    if !prepared.iter().any(|p| p.record.split == "held_out") {
        return;
    }
    body.push_str("<section aria-labelledby=paired-title><h2 id=paired-title>Dezelfde vraag, naast elkaar</h2><p class=help>Kleine verkennende set: 6 vragen × 4 aanpakken, met bekende synthetische bronnen. Hieruit volgt geen betrouwbare algemene winstclaim.</p>");
    for kind in [ExecutionKind::Live, ExecutionKind::Fixture] {
        let selected: Vec<_> = prepared
            .iter()
            .enumerate()
            .filter(|(_, p)| p.record.split == "held_out" && p.record.execution_kind == kind)
            .collect();
        if selected.is_empty() {
            continue;
        }
        let occupied: BTreeSet<_> = selected
            .iter()
            .map(|(_, p)| (&p.record.case_id, p.record.arm.name()))
            .collect();
        let completed = selected
            .iter()
            .filter(|(_, p)| p.record.execution_status == ExecutionStatus::Completed)
            .count();
        write!(body,"<h3>{}</h3><p class=help>{} opgeslagen uitvoeringen · {completed} voltooid · {} niet voltooid · {} ontbrekende combinaties</p><p class=scroll-hint>Schuif de tabel opzij om alle aanpakken te vergelijken →</p><div class=table-scroll tabindex=0 role=region aria-label=\"Gepaarde vergelijking van dezelfde vragen\"><table class=paired><thead><tr><th scope=col>Dezelfde vraag</th>",if kind==ExecutionKind::Live {"Echte uitvoeringen"} else {"Voorbeelden; geen modelkwaliteitsmeting"},selected.len(),selected.len()-completed,questions.cases.len()*4-occupied.len()).unwrap();
        for arm in Arm::ALL {
            write!(body, "<th scope=col>{}</th>", arm_name(arm)).unwrap();
        }
        body.push_str("</tr></thead><tbody>");
        let mut eligible = [0; 4];
        for case in &questions.cases {
            write!(body, "<tr><th scope=row>{}</th>", escape(&case.question)).unwrap();
            let agents: Vec<_> = selected
                .iter()
                .filter(|(_, p)| p.record.case_id == case.id && p.record.arm == Arm::AgenticDense)
                .collect();
            for (arm_index, arm) in Arm::ALL.into_iter().enumerate() {
                let rows: Vec<_> = selected
                    .iter()
                    .filter(|(_, p)| p.record.case_id == case.id && p.record.arm == arm)
                    .collect();
                body.push_str("<td>");
                if rows.is_empty() {
                    body.push_str("<span class=help>Geen uitvoering opgeslagen</span>");
                }
                for (index, p) in &rows {
                    write!(body,"<a class=pair-link href=#question-{index} data-open=question-{index}><span class=\"badge {}\">{}</span></a>",p.outcome.key(),if p.record.execution_status==ExecutionStatus::Completed {p.outcome.label()} else {execution_name(&p.record.execution_status)}).unwrap();
                    write!(
                        body,
                        "<small>{}: {}</small>",
                        if rows.len() == 1 {
                            "Gekozen uitvoering"
                        } else {
                            "Uitvoering in historie"
                        },
                        escape(&p.record.run_id)
                    )
                    .unwrap();
                    if p.record.execution_status == ExecutionStatus::Completed {
                        write!(
                            body,
                            "<strong>Bewijs: {}</strong><small>Zoekacties: {} · tijd: {}</small>",
                            metric_value(&p.measured, "all_hop_fact_coverage"),
                            metric_value(&p.measured, "search_calls"),
                            metric_value(&p.measured, "elapsed_ms")
                        )
                        .unwrap();
                    } else {
                        body.push_str(
                            "<small>Geen geslaagd antwoord; deze fout blijft meetellen.</small>",
                        );
                    }
                    if arm != Arm::AgenticDense
                        && rows.len() == 1
                        && agents.len() == 1
                        && let Some(delta) = paired_delta(agents[0].1, p)
                    {
                        eligible[arm_index] += 1;
                        write!(
                            body,
                            "<small>Verschil met Codex: {}</small>",
                            format!("{delta:+.2}").replace('.', ",")
                        )
                        .unwrap();
                    }
                }
                if rows.len() > 1 {
                    body.push_str("<small>Meerdere uitvoeringen; geen automatische selectie of verschilscore.</small>");
                }
                body.push_str("</td>");
            }
            body.push_str("</tr>");
        }
        body.push_str("</tbody></table></div><details><summary>Welke gepaarde verschillen tellen mee?</summary><p>Het verschil is de bewijsdekking van deze aanpak min die van Codex, voor dezelfde vraag. Geen gemiddelde totaalscore. Alleen unieke voltooide live uitvoeringen met geldige harde controles, een bewijsnoemer en dezelfde vastgelegde model-/programmaconfiguratie krijgen een verschil.</p><ul>");
        for (index, arm) in Arm::ALL
            .into_iter()
            .enumerate()
            .filter(|(_, a)| *a != Arm::AgenticDense)
        {
            write!(
                body,
                "<li>{}: {} van {} vragen vergelijkbaar met Codex</li>",
                arm_name(arm),
                eligible[index],
                questions.cases.len()
            )
            .unwrap();
        }
        body.push_str("</ul><p>Fouten, ontbrekende uitvoeringen, herhalingen en niet-gedefinieerde maten verdwijnen niet uit het overzicht; zij krijgen geen verzonnen verschilscore.</p></details>");
    }
    body.push_str("</section>");
}

fn render_question(body: &mut String, item: &Prepared<'_>, index: usize) {
    let r = item.record;
    let m = &item.measured;
    write!(body, "<details id=question-{index} class=question name=questions data-arm={} data-kind=\"{}\" data-outcome={} data-split=\"{}\"><summary><span><span class=question-text>{}</span><span class=question-meta>{} · {} · {}{}</span></span><span class=\"badge {}\">{}</span></summary><div class=question-body>", r.arm.name(), escape(&r.kind), item.outcome.key(), escape(&r.split), escape(&r.question), arm_name(r.arm), split_name(&r.split), type_name(&r.kind), if r.execution_kind == ExecutionKind::Fixture { " · Voorbeeld" } else { " · Echt" }, item.outcome.key(), if r.execution_status != ExecutionStatus::Completed { execution_name(&r.execution_status) } else { item.outcome.label() }).unwrap();
    body.push_str("<div class=answer-grid><section class=answer-box><h3>Het antwoord</h3>");
    write!(
        body,
        "<p class=help>Generatie: {} · {}</p>",
        execution_name(&r.execution_status),
        if r.execution_kind == ExecutionKind::Fixture {
            "Voorbeeld"
        } else {
            "Echt"
        }
    )
    .unwrap();
    if let Some(answer) = &r.answer {
        write!(body, "<p class=answer>{}</p>", escape(&answer.text)).unwrap();
        if !answer.unknowns.is_empty() {
            write!(
                body,
                "<p class=help><strong>Als onbekend gemeld:</strong> {}</p>",
                escape(&answer.unknowns.join(" · "))
            )
            .unwrap();
        }
    } else {
        write!(
            body,
            "<p>{}. Er is geen antwoord om te beoordelen.</p>",
            execution_name(&r.execution_status)
        )
        .unwrap();
    }
    body.push_str("</section><section class=expected><h3>Verwacht bewijs</h3><ul class=facts>");
    let available: BTreeSet<_> = r
        .offered_sections
        .iter()
        .map(|s| SectionRef {
            source_id: s.source_id.clone(),
            section_id: s.id.clone(),
        })
        .collect();
    for fact in &item.criteria.facts {
        let (class, label) = if fact.evidence_alternatives.is_empty() {
            ("neutral", "Geen positief bewijs in bron")
        } else if fact
            .evidence_alternatives
            .iter()
            .any(|group| group.iter().all(|s| available.contains(s)))
        {
            (
                "good",
                if r.arm == Arm::FullContext {
                    "Context aanwezig"
                } else {
                    "Bewijs gevonden"
                },
            )
        } else {
            ("bad", "Bewijs ontbreekt")
        };
        write!(
            body,
            "<li><span class=\"badge {class}\">{label}</span><p>{}</p>",
            escape(&fact.description)
        )
        .unwrap();
        for (group_index, group) in fact.evidence_alternatives.iter().enumerate() {
            let refs: Vec<_> = group
                .iter()
                .map(|s| {
                    let date = r
                        .manifest
                        .sources
                        .iter()
                        .find(|src| src.id == s.source_id)
                        .map(|s| s.date.as_str())
                        .unwrap_or("Onbekend");
                    format!("{}:{} · {}", s.source_id, s.section_id, date)
                })
                .collect();
            write!(
                body,
                "<small>{}{}</small>",
                if group_index > 0 { "Of: " } else { "" },
                escape(&refs.join(" + "))
            )
            .unwrap();
        }
        body.push_str("</li>");
    }
    body.push_str("</ul><p class=help>Dit meet beschikbaar bronbewijs, niet of het antwoord die feiten juist gebruikt.</p>");
    if !item.criteria.conditions.is_empty()
        || !item.criteria.required_unknowns.is_empty()
        || !item.criteria.forbidden_claims.is_empty()
    {
        body.push_str("<details><summary>Voorwaarden en onbekende gegevens</summary>");
        for (label, values) in [
            ("Voorwaarden", &item.criteria.conditions),
            ("Als onbekend melden", &item.criteria.required_unknowns),
            ("Niet beweren", &item.criteria.forbidden_claims),
        ] {
            if !values.is_empty() {
                write!(body, "<h4>{label}</h4><ul>").unwrap();
                for value in values {
                    write!(body, "<li>{}</li>", escape(value)).unwrap();
                }
                body.push_str("</ul>");
            }
        }
        body.push_str("</details>");
    }
    body.push_str("</section></div><dl class=measure-strip>");
    for (id, label) in [
        ("first_hop_fact_coverage", "Na eerste zoekactie"),
        ("all_hop_fact_coverage", "Na alle zoekacties"),
        ("search_calls", "Zoekacties"),
        ("elapsed_ms", "Tijd per vraag"),
    ] {
        write!(
            body,
            "<div><dt>{label}</dt><dd>{}</dd></div>",
            if r.execution_status == ExecutionStatus::Skipped {
                "N.v.t.".into()
            } else {
                metric_value(m, id)
            }
        )
        .unwrap();
    }
    if r.arm == Arm::FullContext && !r.offered_sections.is_empty() {
        body.push_str("<div><dt>Alle documenten</dt><dd>Context vooraf aangeboden</dd></div>");
    }
    write!(
        body,
        "<div><dt>Tokens</dt><dd>{} in · {} uit</dd></div></dl>",
        metric_value(m, "input_tokens"),
        metric_value(m, "output_tokens")
    )
    .unwrap();
    render_assessment(body, r);
    body.push_str("<details><summary>Harde controles</summary><ul class=checks>");
    for check in &m.hard_checks {
        let label = match check.id.as_str() {
            "execution_completed" => "Uitvoering voltooid",
            "live_trace" => "Complete echte uitvoering",
            "artifact_integrity" => "Geldig bestand, citaten en limieten",
            "canonical_evidence" => "Exacte bron en datum",
            _ => "Zoektrace en context stemmen overeen",
        };
        let fixture = r.execution_kind == ExecutionKind::Fixture && check.id == "live_trace";
        write!(body, "<li><span class=\"badge {}\">{}</span><strong>{label}</strong><p class=help>{}</p></li>", if fixture { "neutral" } else if check.passed { "good" } else { "bad" }, if fixture { "Voorbeeld" } else if check.passed { "In orde" } else { "Niet in orde" }, if fixture { "Dit voorbeeld is geen bewijs van een live modeluitvoering.".into() } else { escape(&check.detail) }).unwrap();
    }
    body.push_str("</ul></details><details><summary>Bekijk de zoekacties en bronnen</summary>");
    if r.arm == Arm::FullContext {
        body.push_str(if r.offered_sections.is_empty() {"<p>Geen aangeboden context vastgelegd. Deze aanpak zoekt niet, maar biedt normaal alle documenten vooraf aan.</p>"} else {"<p>Alle documentsecties zijn vooraf aangeboden. Er is geen zoekactie uitgevoerd.</p>"});
    } else if r.retrieval_steps.is_empty() {
        body.push_str("<p>Geen zoektrace vastgelegd. Bij een onvolledige agentsessie is het werkelijke aantal zoekacties onbekend.</p>");
    }
    for (step_index, step) in r.retrieval_steps.iter().enumerate() {
        write!(body, "<section class=search-step><h4>Zoekactie {} · {}</h4><p>{}</p><small>{} · maximaal {} passages</small>", step_index+1, if step.origin == "mcp" { "Codex / MCP" } else { "Lokale selectie vooraf" }, escape(&step.query), if step.origin == "mcp" { r.tool_calls.get(step_index).map(|c| if c.status == "completed" { "Tool voltooid" } else { "Tool mislukt" }).unwrap_or("Geen toolstatus") } else { "Geen MCP-call" }, step.top_k).unwrap();
        if step.hits.is_empty() {
            body.push_str("<p>Geen passages vastgelegd voor deze zoekpoging.</p>");
        }
        for (rank, hit) in step.hits.iter().enumerate() {
            let s = &hit.section;
            let relevant = item
                .criteria
                .relevant_sections
                .iter()
                .any(|ref_| ref_.source_id == s.source_id && ref_.section_id == s.id);
            let positive = item.criteria.facts.iter().any(|f| {
                f.evidence_alternatives
                    .iter()
                    .flatten()
                    .any(|ref_| ref_.source_id == s.source_id && ref_.section_id == s.id)
            });
            write!(body, "<p class=hit-line><strong>{}. {} · {}</strong> <small>{}:{} · {}</small> <span class=\"badge {}\">{}</span></p>",rank+1,escape(&s.source_title),escape(&s.heading),escape(&s.source_id),escape(&s.id),escape(&s.source_date),if relevant {"good"} else {"neutral"},if positive {"Deel van benodigd bewijs"} else if relevant {"Helpt voorwaarden controleren"} else if item.criteria.relevance_labels_complete {"Niet relevant gelabeld"} else {"Niet volledig gelabeld"}).unwrap();
        }
        body.push_str("</section>");
    }
    for s in &r.offered_sections {
        write!(body, "<details class=source name=source-{index}><summary>{} · {} <small>{}:{} · {}</small></summary><p>{}</p>",escape(&s.source_title),escape(&s.heading),escape(&s.source_id),escape(&s.id),escape(&s.source_date),escape(&s.text)).unwrap();
        if let Some(answer) = &r.answer {
            for c in answer
                .citations
                .iter()
                .filter(|c| c.source_id == s.source_id && c.section_id == s.id)
            {
                write!(
                    body,
                    "<p class=citation><strong>Gebruikt citaat</strong><br>{}</p>",
                    escape(&c.quote)
                )
                .unwrap();
            }
        }
        body.push_str("</details>");
    }
    body.push_str("</details><details><summary>Alle meetwaarden en hun betekenis</summary><dl class=all-metrics>");
    for metric in &m.metrics {
        write!(
            body,
            "<div><dt>{}</dt><dd>{}</dd><p>{}</p>",
            metric_label(&metric.id),
            if r.execution_status == ExecutionStatus::Skipped {
                "N.v.t.".into()
            } else {
                metric_value(m, &metric.id)
            },
            metric_explanation(&metric.id)
        )
        .unwrap();
        if let Some(reason) = &metric.not_applicable_reason {
            write!(
                body,
                "<p class=help>{}</p>",
                if metric.id.ends_with("tokens") {
                    "Tokengebruik is niet beschikbaar."
                } else if r.arm == Arm::FullContext {
                    "Deze aanpak krijgt context vooraf en voert geen retrieval uit."
                } else if reason.contains("Incomplete agent trace") {
                    "De agenttrace is onvolledig; het werkelijke aantal calls is onbekend."
                } else if reason.contains("labels are incomplete")
                    || reason.contains("requires complete")
                {
                    "Niet voor alle passages is een volledig relevantieoordeel vastgelegd."
                } else if reason.contains("No retrieval attempt") {
                    "Er is geen zoekpoging waargenomen voordat de uitvoering stopte."
                } else {
                    "De noemer ontbreekt of is nul; deze maat is hier niet gedefinieerd."
                }
            )
            .unwrap();
        }
        write!(
            body,
            "<small>{}</small>",
            match metric.direction.as_str() {
                "higher_is_better" =>
                    "Hoger is beter binnen deze definitie; geen kans op juistheid.",
                "lower_is_better" => "Lager is sneller; beoordeel dit samen met de kwaliteit.",
                _ => "Beschrijvend; geen zelfstandige slaaggrens.",
            }
        )
        .unwrap();
        body.push_str("</div>");
    }
    body.push_str(
        "</dl></details><details><summary>Versies en uitvoering</summary><dl class=metadata>",
    );
    for (label, value) in [
        ("Vraag-ID", r.case_id.as_str()),
        ("Uitvoering", r.run_id.as_str()),
        ("Vragenset", r.manifest.questions_version.as_str()),
        ("Vraaghash", r.manifest.questions_sha256.as_str()),
        ("Criteriahash", r.manifest.expectations_sha256.as_str()),
        ("Bronhash", r.manifest.corpus_sha256.as_str()),
        ("Referentiedatum", r.manifest.as_of.as_str()),
        ("Model", r.manifest.model.as_str()),
        ("Codex-versie", r.manifest.client_version.as_str()),
        ("Embeddingmodel", r.manifest.embedding_model.as_str()),
        (
            "Embeddingrevisie",
            r.manifest
                .embedding_revision
                .as_deref()
                .unwrap_or("Niet vastgelegd"),
        ),
        (
            "Generatieprompt",
            r.manifest.generation_prompt_version.as_str(),
        ),
        ("Prompthash", r.manifest.generation_prompt_sha256.as_str()),
        ("Rubric-versie", r.manifest.rubric_version.as_str()),
        ("Rubrichash", r.manifest.rubric_sha256.as_str()),
        ("Runtimehash", r.manifest.runtime_sha256.as_str()),
        ("Programmaversie", r.manifest.binary_sha256.as_str()),
    ] {
        write!(body, "<dt>{label}</dt><dd>{}</dd>", escape(value)).unwrap();
    }
    write!(
        body,
        "<dt>Uitvoeringsrecord gemaakt (UTC)</dt><dd><time data-epoch-ms={}>{} Unix-ms</time></dd>",
        r.manifest.created_at_unix_ms, r.manifest.created_at_unix_ms
    )
    .unwrap();
    write!(body,"<dt>Zoeklimieten</dt><dd>top-k {} · maximaal {} MCP-calls</dd><dt>Uitvoeringslimieten</dt><dd>maximaal {} s per modelsessie · concurrency {}</dd>",r.manifest.limits.top_k,r.manifest.limits.max_search_calls,r.manifest.limits.per_call_seconds,r.manifest.limits.concurrency).unwrap();
    for file in &r.manifest.embedding_files {
        write!(
            body,
            "<dt>{}</dt><dd>{}</dd>",
            escape(&file.name),
            escape(&file.sha256)
        )
        .unwrap();
    }
    if let Some(b) = &r.manifest.budget {
        write!(body,"<dt>Globale budgetregistratie</dt><dd>{} · {} generaties / {} beoordelingen gereserveerd</dd>",escape(&b.id),b.generation_sessions_used,b.grading_sessions_used).unwrap();
    }
    body.push_str("</dl>");
    if !r.manifest.reproducibility_limitations.is_empty() {
        body.push_str("<h4>Grenzen van reproduceerbaarheid</h4><ul>");
        for limitation in &r.manifest.reproducibility_limitations {
            write!(body, "<li>{}</li>", escape(limitation)).unwrap();
        }
        body.push_str("</ul>");
    }
    body.push_str("</details></div></details>");
}

fn render_assessment(body: &mut String, record: &ExperimentRecord) {
    body.push_str("<section class=assessment><h3>Afzonderlijk AI-oordeel</h3>");
    if let Some(j) = &record.judgement {
        let status = match j["execution_status"].as_str() {
            Some("completed") => "Voltooid",
            Some("timed_out") => "Time-out",
            Some("cancelled") => "Geannuleerd",
            Some("skipped") => "Niet uitgevoerd",
            _ => "Beoordeling mislukt",
        };
        let kind = if j["execution_kind"] == "live" {
            "Echte modelbeoordeling"
        } else {
            "Voorbeeldbeoordeling"
        };
        write!(body, "<p class=help>Beoordelaar: {status} · {kind}</p>").unwrap();
        let verdict = match j["assessment"]["overall"].as_str() {
            Some("good") => "Goed",
            Some("bad") => "Fout",
            _ => "Beoordeling nodig",
        };
        write!(
            body,
            "<p><strong>{verdict} · voorlopig</strong> · Geen menselijke ijking</p>"
        )
        .unwrap();
        let usage = |field: &str| {
            j["usage"][field]
                .as_u64()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "Onbekend".into())
        };
        write!(
            body,
            "<p class=help>Beoordelingstijd: {} s · tokens {} in / {} uit</p>",
            format!(
                "{:.2}",
                j["elapsed_ms"].as_u64().unwrap_or(0) as f64 / 1000.0
            )
            .replace('.', ","),
            usage("input_tokens"),
            usage("output_tokens")
        )
        .unwrap();
        if let (Some(n), Some(d)) = (
            j["semantic_numerator"].as_u64(),
            j["semantic_denominator"].as_u64(),
        ) {
            write!(
                body,
                "<p>{n} van {d} beoordeelde criteria als goed; {} criteria nog te beoordelen.</p>",
                j["review_needed_criteria"].as_u64().unwrap_or(0)
            )
            .unwrap();
        }
        if let Some(criteria) = j["assessment"]["criteria"].as_array() {
            body.push_str(
                "<details><summary>Bekijk het AI-oordeel per criterium</summary><ul class=checks>",
            );
            for c in criteria {
                let id = c["criterion_id"].as_str().unwrap_or("Onbekend");
                let title = match id {
                    "correctness" => "Klopt het antwoord?",
                    "completeness" => "Is de vraag volledig beantwoord?",
                    "relevance" => "Blijft het antwoord bij de vraag?",
                    "source_support" => "Komen claims uit aangeboden bronnen?",
                    "appropriate_unknowns" => "Zijn onbekende gegevens eerlijk aangegeven?",
                    _ => id,
                };
                write!(
                    body,
                    "<li><strong>{}</strong> · {}<p>{}</p>",
                    escape(title),
                    match c["verdict"].as_str() {
                        Some("good") => "Goed",
                        Some("bad") => "Fout",
                        _ => "Beoordeling nodig",
                    },
                    escape(c["reason"].as_str().unwrap_or("Geen toelichting"))
                )
                .unwrap();
                if let Some(f) = c["answer_fragment"].as_str() {
                    write!(body, "<blockquote>{}</blockquote>", escape(f)).unwrap();
                }
                if let Some(evidence) = c["source_evidence"].as_object() {
                    let source = evidence
                        .get("source_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Onbekend");
                    let section = evidence
                        .get("section_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Onbekend");
                    let date = record
                        .manifest
                        .sources
                        .iter()
                        .find(|s| s.id == source)
                        .map(|s| s.date.as_str())
                        .unwrap_or("Onbekend");
                    let offered = record
                        .offered_sections
                        .iter()
                        .any(|s| s.source_id == source && s.id == section);
                    write!(
                        body,
                        "<p class=help>{}:{} · {} · {}</p><blockquote>{}</blockquote>",
                        escape(source),
                        escape(section),
                        escape(date),
                        if offered {
                            "Aan generator aangeboden"
                        } else {
                            "Referentiebron; niet aan generator aangeboden"
                        },
                        escape(evidence.get("quote").and_then(|v| v.as_str()).unwrap_or(""))
                    )
                    .unwrap();
                }
                body.push_str("</li>");
            }
            body.push_str("</ul></details>");
        }
        if let Some(b) = j["budget"].as_object() {
            write!(body,"<details><summary>Budgetstand bij deze beoordeling</summary><p>{} generaties / {} beoordelingen gereserveerd in deze globale registratie; geen som per vraag.</p></details>",b.get("generation_sessions_used").and_then(|v|v.as_u64()).unwrap_or(0),b.get("grading_sessions_used").and_then(|v|v.as_u64()).unwrap_or(0)).unwrap();
        }
        body.push_str("<details><summary>Versie van de beoordelaar</summary><dl class=metadata>");
        if let Some(started) = j["started_at_unix_ms"].as_u64() {
            write!(body,"<dt>Beoordeling gestart (UTC)</dt><dd><time data-epoch-ms={started}>{started} Unix-ms</time></dd>").unwrap();
        } else {
            body.push_str("<dt>Beoordeling gestart</dt><dd>Niet vastgelegd</dd>");
        }
        for (label, key) in [
            ("Model", "model"),
            ("Codex-versie", "client_version"),
            ("Rubric-versie", "rubric_version"),
            ("Rubrichash", "rubric_sha256"),
            ("Prompthash", "prompt_sha256"),
            ("Runtimehash", "runtime_sha256"),
        ] {
            write!(
                body,
                "<dt>{label}</dt><dd>{}</dd>",
                escape(j[key].as_str().unwrap_or("Niet vastgelegd"))
            )
            .unwrap();
        }
        body.push_str("</dl></details>");
    } else {
        body.push_str("<p>Nog geen afzonderlijke AI-beoordeling opgeslagen. Bewijsdekking is geen antwoordoordeel.</p>");
    }
    body.push_str("</section>");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn examples() -> Vec<ExperimentRecord> {
        include_str!("../../fixtures/experiments/report-example.jsonl")
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn offline_report_explains_variants_examples_unknown_usage_and_actual_sources() {
        let rows = examples();
        for row in &rows {
            super::super::runner::validate_record(row).unwrap();
        }
        let html = render(&rows).unwrap();
        for expected in [
            "Codex zoekt verder",
            "Woordzoeken (BM25)",
            "Betekeniszoeken (MiniLM)",
            "Alle documenten",
            "Voorbeeldresultaten",
            "Geen menselijke ijking",
            "Verwacht bewijs",
            "ACT-TUIN-001",
            "2026-10-20",
            "Onbekend",
            "Niet uitgevoerd",
            "Time-out",
        ] {
            assert!(html.contains(expected), "missing {expected}");
        }
        assert!(!html.contains("https://"));
    }

    #[test]
    fn untrusted_answers_are_text_and_cannot_close_the_script_or_create_elements() {
        let mut rows = examples();
        rows[0].answer.as_mut().unwrap().text = "</script><script>window.reportCanary=true</script><img src=x onerror=alert(1)> & \"quote\"".into();
        let html = render(&rows).unwrap();
        assert!(html.contains("&lt;/script&gt;&lt;script&gt;window.reportCanary=true"));
        assert!(html.contains("&lt;img src=x onerror=alert(1)&gt; &amp; &quot;quote&quot;"));
        assert_eq!(html.matches("<script").count(), 1);
        assert!(!html.contains("<img"));
    }

    #[test]
    fn changed_questions_or_saved_measurements_cannot_silently_change_the_report() {
        let mut rows = examples();
        rows[0].question = "Invented question".into();
        assert!(render(&rows).is_err());
        assert!(render(&[]).is_err());
        let mut rows = examples();
        rows.push(rows[0].clone());
        assert!(render(&rows).is_err());
    }

    #[test]
    fn grader_status_and_kind_remain_visible_and_unresolved_criteria_are_not_hidden() {
        let mut row = examples().remove(0);
        row.judgement = Some(
            serde_json::json!({"execution_status":"timed_out","execution_kind":"fixture","assessment":null,"elapsed_ms":1000,"usage":null}),
        );
        let mut html = String::new();
        render_assessment(&mut html, &row);
        assert!(html.contains("Time-out"));
        assert!(html.contains("Voorbeeldbeoordeling"));
        row.judgement = Some(
            serde_json::json!({"execution_status":"completed","execution_kind":"live","assessment":{"overall":"review_needed","criteria":[]},"semantic_numerator":1,"semantic_denominator":1,"review_needed_criteria":9,"elapsed_ms":1000,"usage":null}),
        );
        let mut html = String::new();
        render_assessment(&mut html, &row);
        assert!(
            html.contains("1 van 1 beoordeelde criteria als goed; 9 criteria nog te beoordelen")
        );
        assert!(html.contains("Beoordeling nodig"));
        assert!(!html.contains("100%"));
    }

    fn directory() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "coa-report-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn write_report_never_overwrites_inputs_and_invalid_inputs_keep_existing_output() {
        let dir = directory();
        let input = dir.join("example.jsonl");
        let output = dir.join("report.html");
        let fixture = include_str!("../../fixtures/experiments/report-example.jsonl");
        std::fs::write(&input, fixture).unwrap();
        assert_eq!(
            write_report(std::slice::from_ref(&input), &output).unwrap(),
            6
        );
        assert!(
            std::fs::read_to_string(&output)
                .unwrap()
                .contains("Voorbeeldresultaten")
        );
        assert!(write_report(std::slice::from_ref(&input), &input).is_err());
        assert_eq!(std::fs::read_to_string(&input).unwrap(), fixture);
        std::fs::write(&input, "{PRIVATE-CANARY}").unwrap();
        std::fs::write(&output, "previous report").unwrap();
        let error = write_report(std::slice::from_ref(&input), &output).unwrap_err();
        assert!(!error.contains("PRIVATE-CANARY"));
        assert_eq!(std::fs::read_to_string(&output).unwrap(), "previous report");
        std::fs::write(&input, "").unwrap();
        assert!(write_report(&[input], &output).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn preexisting_temporary_file_is_not_removed_on_a_failed_write() {
        let dir = directory();
        let input = dir.join("example.jsonl");
        let output = dir.join("report.html");
        std::fs::write(
            &input,
            include_str!("../../fixtures/experiments/report-example.jsonl"),
        )
        .unwrap();
        let temp = dir.join(format!(".evaluation-report-{}.tmp", std::process::id()));
        std::fs::write(&temp, "belongs to another write").unwrap();
        assert!(write_report(&[input], &output).is_err());
        assert_eq!(
            std::fs::read_to_string(&temp).unwrap(),
            "belongs to another write"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn paired_table_keeps_all_six_cases_and_distinguishes_missing_runs_from_zero_coverage() {
        let mut row = examples().remove(0);
        let (questions, labels) = catalog().unwrap().remove(1);
        let case = &questions.cases[0];
        row.case_id = case.id.clone();
        row.family = case.family.clone();
        row.split = case.split.clone();
        row.kind = case.kind.clone();
        row.question = case.question.clone();
        row.manifest.questions_version = questions.dataset_version.clone();
        row.manifest.questions_sha256 = dataset::questions_hash(&questions).unwrap();
        row.manifest.expectations_version = labels.dataset_version.clone();
        row.manifest.expectations_sha256 =
            crate::manifest::hash(&serde_json::to_vec(&labels).unwrap());
        for (call, step) in row.tool_calls.iter_mut().zip(&mut row.retrieval_steps) {
            call.query = case.question.clone();
            step.query = case.question.clone();
            call.result.as_mut().unwrap()["question"] = serde_json::json!(case.question);
        }
        let html = render(&[row]).unwrap();
        assert!(html.contains("Dezelfde vraag, naast elkaar"));
        assert!(html.contains("6 vragen × 4 aanpakken"));
        assert!(html.contains("23 ontbrekende combinaties"));
        assert!(html.contains("0 van 4 (0,00)"));
        assert!(html.contains("Geen uitvoering opgeslagen"));
        for question in questions.cases {
            assert!(html.contains(&escape(&question.question)));
        }
    }

    #[test]
    fn paired_difference_uses_known_fractions_and_refuses_a_different_case_or_runtime() {
        let rows = examples();
        let datasets = catalog().unwrap();
        let mut a = rows[0].clone();
        let mut b = rows[1].clone();
        let criteria = bind(&a, &datasets).unwrap();
        let mut am = metrics::evaluate(&a, criteria).unwrap();
        let mut bm = metrics::evaluate(&b, criteria).unwrap();
        // Instrument inputs only: this does not create a live result artifact.
        a.execution_kind = ExecutionKind::Live;
        b.execution_kind = ExecutionKind::Live;
        am.hard_checks_passed = true;
        bm.hard_checks_passed = true;
        let agent = Prepared {
            record: &a,
            criteria,
            measured: am,
            outcome: Outcome::Review,
        };
        let baseline = |r| Prepared {
            record: r,
            criteria,
            measured: bm.clone(),
            outcome: Outcome::Review,
        };
        assert_eq!(paired_delta(&agent, &baseline(&b)), Some(-0.5));
        let mut other_case = b.clone();
        other_case.case_id = "P07".into();
        assert_eq!(paired_delta(&agent, &baseline(&other_case)), None);
        let mut other_runtime = b.clone();
        other_runtime.manifest.runtime_sha256 = "9".repeat(64);
        assert_eq!(paired_delta(&agent, &baseline(&other_runtime)), None);
    }
}
