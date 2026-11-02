(() => {
  "use strict";
  const data = JSON.parse(document.getElementById("report-data").textContent);
  const byId = (id) => document.getElementById(id);
  const rows = data.rows;
  byId("empty-state").hidden = rows.length > 0;
  byId("lab").hidden = rows.length === 0;
  if (!rows.length) return;

  const methods = {
    dense: ["Vector", "Zoeken op betekenis"],
    bm25: ["Keyword · BM25", "Zoeken op woorden"],
    hybrid_rrf: ["Hybrid · RRF", "Ranglijsten combineren"],
    hybrid_weighted: ["Hybrid · gewogen", "Geschaalde scores combineren"]
  };
  const profiles = {
    documents_sections: ["Documenten", "Passages"],
    documents_activities: ["Documenten", "Activiteiten"],
    sqlite_sections: ["SQLite", "Passages"],
    sqlite_activities: ["SQLite", "Activiteiten"]
  };
  const kinds = {
    multiple_facts: "Meerdere feiten", combined_conditions: "Voorwaarden combineren",
    missing_information: "Ontbrekende informatie", paraphrase: "Anders geformuleerd",
    stale_information: "Actualiteit", all_matches: "Alle passende activiteiten",
    no_matching_activity: "Geen passende activiteit"
  };
  const verdicts = { good: "Goed", bad: "Onvoldoende", review_needed: "Nader bekijken" };
  const metricLabels = {
    evidence_coverage: "Volledige verwachte feiten", section_precision: "Relevante passages / opgehaalde passages",
    section_recall: "Gevonden / alle relevante passages", first_relevant_reciprocal_rank: "Eerste relevante zoekresultaat",
    retrieval_ms: "Alleen lokaal zoeken", estimated_context_tokens: "Geschatte contextgrootte",
    local_search_calls: "Lokale zoekacties", answer_correctness: "Antwoordjuistheid",
    answer_citation_validity: "Bronverwijzingen in een antwoord"
  };
  const checkLabels = {
    execution_completed: "Zoekactie uitgevoerd", record_identity: "Vraag en instellingen kloppen",
    canonical_evidence: "Bronpassages ongewijzigd", retrieval_trace: "Zoekspoor klopt",
    context_budget: "Contextlimiet gerespecteerd", storage_layout: "Opgeslagen structuur klopt",
    typed_filters: "Filters op gecontroleerde feiten", measured_execution: "Echte lokale meting"
  };
  const methodIds = Object.keys(methods);
  const profileIds = Object.keys(profiles);
  const state = { selected: null, localReviews: new Map(), storageAvailable: true };
  const storageKey = "coa-retrieval-reviews-v1:" + data.artifact_id;
  const rowById = new Map(rows.map((row) => [row.record.run_id, row]));

  function element(tag, className, text) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined && text !== null) node.textContent = String(text);
    return node;
  }
  function number(value, digits = 3) {
    return new Intl.NumberFormat("nl-NL", { maximumFractionDigits: digits }).format(value);
  }
  function milliseconds(value) {
    if (value === null || value === undefined) return "N.v.t.";
    if (value > 0 && value < 0.001) return "< 0,001 ms";
    return number(value) + " ms";
  }
  function timing(row) {
    return row.record.execution_kind === "fixture" ? "Oefenrun" : milliseconds(row.record.timings.retrieval_ms);
  }
  function methodName(row) { return methods[row.record.config.retrieval.method][0]; }
  function profileName(row) { return profiles[row.record.config.profile].join(" / "); }
  function deterministic(row) { return row.assessments.find((assessment) => assessment.kind === "deterministic")?.deterministic; }
  function manual(row) { return state.localReviews.get(row.record.run_id) || row.assessments.find((assessment) => assessment.kind === "manual"); }
  function metric(row, id) { return deterministic(row)?.metrics.find((value) => value.id === id); }
  function badge(text, style) { return element("span", "badge " + style, text); }
  function settingsKey(row) {
    const { method, ...settings } = row.record.config.retrieval;
    return JSON.stringify(settings);
  }
  function settingsName(row) {
    const config = row.record.config.retrieval;
    return "Top " + config.top_k + " · " + config.candidate_k + " kandidaten · " +
      config.context_token_limit + " contextschatting · " +
      (config.selection === "activity_expansion" ? "activiteituitbreiding" : "passages") +
      " · filters " + (config.apply_filters ? "aan" : "uit") + " · spreiding " + (config.mmr ? "aan" : "uit");
  }
  function addOption(select, value, text) {
    const option = element("option", "", text);
    option.value = value;
    select.append(option);
  }
  function currentRows(includeAllQuestions = false) {
    return rows.filter((row) =>
      (includeAllQuestions || row.record.case_id === byId("question-select").value) &&
      (byId("method-select").value === "all" || row.record.config.retrieval.method === byId("method-select").value) &&
      (byId("profile-select").value === "all" || row.record.config.profile === byId("profile-select").value) &&
      settingsKey(row) === byId("settings-select").value);
  }
  function grade(row) {
    if (row.record.execution_status !== "completed") return { text: row.record.execution_status === "unsupported" ? "Niet beschikbaar" : "Mislukt", style: "failed" };
    if (byId("assessment-select").value === "ai") return { text: "Gepauzeerd", style: "unavailable" };
    if (byId("assessment-select").value === "manual") {
      const review = manual(row);
      if (!review?.manual || review.status !== "completed") return { text: "Onbeoordeeld", style: "unavailable" };
      return { text: verdicts[review.manual.verdict], style: review.manual.verdict === "good" ? "covered" : review.manual.verdict === "bad" ? "failed" : "partial" };
    }
    const coverage = metric(row, "evidence_coverage");
    if (coverage?.value === null || coverage?.value === undefined) return { text: "N.v.t.", style: "unavailable" };
    const text = coverage.numerator !== null && coverage.denominator !== null ?
      coverage.numerator + "/" + coverage.denominator + " feiten" : number(coverage.value, 2);
    return { text, style: coverage.value === 1 ? "covered" : "partial" };
  }
  function validLocalReview(review) {
    const row = rowById.get(review?.run_id);
    if (!row || review.schema_version !== 1 || review.kind !== "manual" ||
      review.retrieval_sha256 !== row.record_sha256 || review.case_id !== row.record.case_id ||
      review.config_id !== row.record.config_id || review.record_hash_policy !== data.record_hash_policy ||
      review.assessor_version !== data.assessor_version || review.deterministic !== null || review.expectations_sha256 !== null) return false;
    if (review.status === "unreviewed") return review.manual === null;
    return review.status === "completed" && review.manual &&
      Object.hasOwn(verdicts, review.manual.verdict) && typeof review.manual.notes === "string" &&
      review.manual.notes.length <= 4000 && Number.isSafeInteger(review.manual.reviewed_at_unix_ms) && review.manual.reviewed_at_unix_ms > 0;
  }
  function loadReviews() {
    let raw;
    try {
      raw = localStorage.getItem(storageKey);
    } catch (_) { state.storageAvailable = false; return; }
    try {
      const saved = JSON.parse(raw || "null");
      if (saved?.schema_version === 1 && saved.artifact_id === data.artifact_id && Array.isArray(saved.reviews)) {
        saved.reviews.filter(validLocalReview).forEach((review) => state.localReviews.set(review.run_id, review));
      }
    } catch (_) { /* An invalid browser draft does not change the stored experiment. */ }
  }
  function persistReviews() {
    try {
      localStorage.setItem(storageKey, JSON.stringify({
        schema_version: 1, artifact_id: data.artifact_id, reviews: [...state.localReviews.values()]
      }));
    } catch (_) { state.storageAvailable = false; }
    byId("storage-notice").hidden = state.storageAvailable;
    byId("storage-notice").textContent = "Je browser bewaart de beoordelingen niet. Exporteer ze voordat je de pagina sluit.";
  }
  function newReview(row, verdict, notes) {
    const now = Date.now();
    return {
      schema_version: 1, assessment_id: "human-local-" + row.record.run_id + "-" + now,
      run_id: row.record.run_id, case_id: row.record.case_id, config_id: row.record.config_id,
      retrieval_sha256: row.record_sha256, record_hash_policy: data.record_hash_policy,
      kind: "manual", status: verdict ? "completed" : "unreviewed", assessor_version: data.assessor_version,
      created_at_unix_ms: now, expectations_sha256: null, deterministic: null,
      manual: verdict ? { verdict, notes, reviewed_at_unix_ms: now } : null,
      note: verdict ? "Explicit human review entered in the local HTML report." : "Human review cleared in the local HTML report."
    };
  }
  function exportedReviews() {
    return rows.map((row) => manual(row)).filter((review) =>
      review && (review.status === "completed" || state.localReviews.has(review.run_id)));
  }
  function renderGrid(visible) {
    const host = byId("comparison-grid");
    host.replaceChildren();
    const question = rows.find((row) => row.record.case_id === byId("question-select").value)?.record;
    byId("comparison-heading").textContent = question?.question || "Geen vraag geselecteerd";
    byId("question-kind").textContent = question ? question.case_id + " · " + (kinds[question.kind] || question.kind) : "";
    byId("selection-count").textContent = visible.length + " opgeslagen uitvoeringen";
    const type = byId("assessment-select").value;
    byId("assessment-notice").hidden = type === "deterministic";
    byId("assessment-notice").textContent = type === "ai" ?
      "AI-beoordeling is gepauzeerd. Je bekijkt dezelfde opgeslagen zoekresultaten; er wordt geen model aangeroepen." :
      "Jouw oordeel begint leeg. Open een resultaat, bekijk de passages en voeg je beoordeling toe.";
    if (!visible.length) {
      host.append(element("p", "notice", "Geen opgeslagen uitvoering voor deze combinatie. Kies andere instellingen."));
      return;
    }
    const shownMethods = byId("method-select").value === "all" ? methodIds : [byId("method-select").value];
    const shownProfiles = byId("profile-select").value === "all" ? profileIds : [byId("profile-select").value];
    const populatedCells = new Set(visible.map((row) => row.record.config.profile + "/" + row.record.config.retrieval.method)).size;
    const missingCells = shownMethods.length * shownProfiles.length - populatedCells;
    byId("selection-count").textContent = visible.length + " opgeslagen uitvoeringen" +
      (missingCells ? " · " + missingCells + " combinaties niet uitgevoerd" : "");
    const scroll = element("div", "comparison-scroll");
    scroll.tabIndex = 0;
    scroll.setAttribute("role", "region");
    scroll.setAttribute("aria-label", "Vergelijking van zoekmethoden en structuren; horizontaal te scrollen");
    const table = element("table", "comparison-table");
    table.setAttribute("aria-describedby", "comparison-help");
    const head = element("thead"), header = element("tr");
    header.append(element("th", "", "Opslag / structuur"));
    shownMethods.forEach((id) => {
      const th = element("th", "", methods[id][0]);
      th.scope = "col";
      th.append(element("small", "", methods[id][1]));
      header.append(th);
    });
    head.append(header);
    table.append(head);
    const body = element("tbody");
    shownProfiles.forEach((id) => {
      const tr = element("tr"), heading = element("th", "", profiles[id][0]);
      heading.scope = "row";
      heading.append(element("small", "", profiles[id][1]));
      tr.append(heading);
      shownMethods.forEach((method) => {
        const td = element("td");
        const cellRows = visible.filter((row) => row.record.config.profile === id && row.record.config.retrieval.method === method);
        if (!cellRows.length) td.append(element("span", "no-cell", "Niet uitgevoerd"));
        cellRows.forEach((row, index) => {
          const status = grade(row), button = element("button", "result-choice " + status.style);
          button.type = "button";
          button.dataset.runId = row.record.run_id;
          button.setAttribute("aria-pressed", String(state.selected === row.record.run_id));
          button.setAttribute("aria-label", methods[method][0] + ", " + profiles[id].join(" / ") + ": " + status.text + (cellRows.length > 1 ? ", uitvoering " + (index + 1) : ""));
          button.append(element("strong", "", status.text), element("small", "", timing(row)));
          if (cellRows.length > 1) button.append(element("small", "", "Uitvoering " + (index + 1)));
          button.addEventListener("click", () => {
            state.selected = row.record.run_id;
            renderGrid(visible);
            renderDetail();
            byId("detail-heading").focus({ preventScroll: true });
            byId("result-detail").scrollIntoView({ behavior: "instant", block: "start" });
          });
          td.append(button);
        });
        tr.append(td);
      });
      body.append(tr);
    });
    table.append(body); scroll.append(table); host.append(scroll);
  }
  function sourceId(index) { return "retrieved-passage-" + index; }
  function refsLabel(refs) { return refs.map((ref) => ref.source_id + ":" + ref.section_id).join(" + "); }
  function renderFacts(row) {
    const host = byId("expected-facts");
    host.replaceChildren();
    const assessment = deterministic(row);
    if (!assessment) {
      host.append(element("p", "notice", "Er is nog geen deterministische beoordeling opgeslagen. Bereken die op deze uitvoeringen om de verwachtingen te bekijken."));
      return;
    }
    if (!assessment.expected_facts.length) host.append(element("p", "help", "Er zijn geen positieve bronfeiten vastgelegd voor deze vraag."));
    const hits = row.record.outcome?.hits || [];
    assessment.expected_facts.forEach((fact) => {
      const card = element("article", "fact");
      card.append(badge(fact.covered === true ? "Ondersteund" : fact.covered === false ? "Nog niet compleet" : "N.v.t.",
        fact.covered === true ? "covered" : fact.covered === false ? "partial" : "unavailable"));
      card.append(element("p", "", fact.criterion.description));
      const refs = element("div", "fact-references");
      if (fact.covered === false) {
        fact.missing_by_alternative.forEach((alternative, index) => {
          refs.append(element("span", "", (index > 0 ? "Of ontbreekt: " : "Ontbreekt: ") + refsLabel(alternative)));
        });
      } else if (fact.covered === true) {
        const supporting = fact.criterion.evidence_alternatives.find((alternative) =>
          alternative.every((ref) => hits.some((hit) => hit.section.source_id === ref.source_id && hit.section.id === ref.section_id))) || [];
        supporting.forEach((ref) => {
          const index = hits.findIndex((hit) => hit.section.source_id === ref.source_id && hit.section.id === ref.section_id);
          const link = element("a", "", ref.source_id + ":" + ref.section_id);
          link.href = "#" + sourceId(index);
          refs.append(link);
        });
      } else refs.append(element("span", "", !fact.criterion.evidence_alternatives.length ?
        "Geen positief bronfeit om te vinden. Of een antwoord terecht ‘onbekend’ zegt, vraagt een aparte beoordeling." :
        "Niet te beoordelen: de zoekactie of de controles op de brongegevens zijn niet geldig."));
      card.append(refs); host.append(card);
    });
    if (assessment.required_unknowns.length) {
      const section = element("div", "conditions");
      section.append(element("h4", "", "Dit moet onbekend blijven"));
      const list = element("ul");
      assessment.required_unknowns.forEach((text) => list.append(element("li", "", text)));
      section.append(list, element("p", "help", "Of een antwoord dit goed aangeeft, beoordelen we hier niet."));
      host.append(section);
    }
    if (assessment.conditions.length) {
      const details = element("details", "conditions");
      details.append(element("summary", "", "Voorwaarden bij deze vraag"));
      const list = element("ul");
      assessment.conditions.forEach((text) => list.append(element("li", "", text)));
      details.append(list); host.append(details);
    }
  }
  function appendDefinition(list, title, value, note) {
    list.append(element("dt", "", title));
    const dd = element("dd", "", value);
    if (note) dd.append(element("small", "", note));
    list.append(dd);
  }
  function renderSources(row) {
    const host = byId("retrieved-passages"), hits = row.record.outcome?.hits || [];
    host.replaceChildren();
    byId("passage-count").textContent = hits.length + " passages met hun oorspronkelijke bron en datum.";
    if (!hits.length) host.append(element("p", "notice", row.record.execution_status === "completed" ?
      "Deze zoekactie gaf geen passages terug." : "Deze uitvoering heeft geen zoekresultaten."));
    hits.forEach((hit, index) => {
      const source = element("article", "source-passage");
      source.id = sourceId(index);
      source.dataset.activity = hit.section.source_id;
      source.append(element("h4", "", hit.section.source_title + " · " + hit.section.heading));
      source.append(element("span", "source-meta", hit.section.source_id + ":" + hit.section.id + " · brondatum " +
        hit.section.source_date + (hit.inclusion === "activity_expansion" ? " · aangevuld vanuit dezelfde activiteit" : " · zoekresultaat")));
      source.append(element("p", "", hit.section.text));
      if (hit.scores) {
        const details = element("details");
        details.append(element("summary", "", "Zoekscores bekijken"));
        const list = element("dl", "definition-list");
        appendDefinition(list, "Rangscore", number(hit.scores.rank_score, 6));
        if (hit.scores.dense_similarity !== null) appendDefinition(list, "Vectorovereenkomst", number(hit.scores.dense_similarity, 4));
        if (hit.scores.bm25_score !== null) appendDefinition(list, "BM25", number(hit.scores.bm25_score, 4));
        details.append(list, element("p", "help", "Scores ordenen resultaten. Het zijn geen kansen dat informatie klopt."));
        source.append(details);
      }
      host.append(source);
    });
  }
  function renderTechnical(row) {
    const host = byId("technical-content"), record = row.record, assessment = deterministic(row);
    host.replaceChildren();
    host.append(element("h3", "", "Voorbereiding en zoektijd"));
    const times = element("dl", "definition-list");
    [["Model laden", record.preparation.model_load_ms], ["Documentembeddings", record.preparation.document_embedding_ms],
      ["Opslag bijwerken", record.preparation.storage_sync_ms], ["Opslag laden", record.preparation.storage_load_ms],
      ["Vraagembedding", record.timings.query_embedding_ms], ["Alleen lokaal zoeken", record.timings.retrieval_ms]]
      .forEach(([label, value]) => appendDefinition(times, label, record.execution_kind === "fixture" ? "N.v.t. · oefenrun" : milliseconds(value)));
    appendDefinition(times, "Voorbereiding", record.preparation.preparation_id || "Niet uitgevoerd",
      "Een voorbereiding kan door meerdere vragen worden gedeeld; tel deze tijd niet per vraag opnieuw op.");
    appendDefinition(times, "Embeddingcache documenten", record.preparation.document_cache_hits + " hergebruikt / " + record.preparation.document_cache_misses + " nieuw");
    appendDefinition(times, "Embeddingcache vraag", record.timings.query_embedding_cache_hit === null ? "N.v.t." : record.timings.query_embedding_cache_hit ? "Hergebruikt" : "Nieuw berekend");
    host.append(times);
    if (record.shared_preparation) {
      host.append(element("h3", "", "Gedeelde voorbereiding"),
        element("p", "help", "Deze voorbereiding kan door meerdere combinaties worden gebruikt. Vergelijk lokale zoektijd apart en tel gedeeld model- of embeddingwerk niet opnieuw per combinatie."),
        element("pre", "", JSON.stringify(record.shared_preparation, null, 2)));
    }
    if (assessment) {
      host.append(element("h3", "", "Deterministische controles"));
      const checks = element("ul", "checks");
      assessment.hard_checks.forEach((check) => {
        const item = element("li");
        item.append(badge(check.passed ? "Klopt" : "Controle nodig", check.passed ? "covered" : "partial"),
          element("span", "", checkLabels[check.id] || check.id), element("p", "", check.detail));
        checks.append(item);
      });
      host.append(checks, element("h3", "", "Alle metingen"));
      const metrics = element("dl", "definition-list");
      assessment.metrics.forEach((value) => {
        const denominator = value.numerator !== null && value.denominator !== null ? " (" + value.numerator + "/" + value.denominator + ")" : "";
        appendDefinition(metrics, metricLabels[value.id] || value.id, value.value === null ? "N.v.t." : number(value.value, 4) + denominator,
          value.not_applicable_reason || value.definition);
      });
      host.append(metrics);
    }
    host.append(element("h3", "", "Vastgelegde uitvoering"));
    const identity = element("dl", "definition-list");
    appendDefinition(identity, "Uitvoering", record.run_id);
    appendDefinition(identity, "Configuratie", record.config_id);
    appendDefinition(identity, "Werkelijke opslag", record.preparation.actual_profile ? profiles[record.preparation.actual_profile].join(" / ") : "Niet uitgevoerd");
    appendDefinition(identity, "Recordhash", row.record_sha256);
    appendDefinition(identity, "Bronhash", record.preparation.corpus_identity?.source_sha256 || record.dataset.corpus_sha256 || "Onbekend");
    host.append(identity);
    const config = element("pre", "", JSON.stringify({
      retrieval: record.config.retrieval, requested_filters: record.request_filters,
      applied_filters: record.outcome?.filters || null, omitted: record.outcome?.omitted || [],
      work: record.outcome?.work || null
    }, null, 2));
    host.append(element("h3", "", "Instellingen en begrenzingen"), config);
  }
  function renderManual(row) {
    byId("manual-review").hidden = byId("assessment-select").value !== "manual";
    const review = manual(row), payload = review?.status === "completed" ? review.manual : null;
    const available = row.record.execution_status === "completed";
    document.querySelectorAll("[data-verdict]").forEach((button) => {
      button.setAttribute("aria-pressed", String(payload?.verdict === button.dataset.verdict));
      button.disabled = !available;
    });
    byId("review-notes").value = payload?.notes || "";
    byId("review-notes").disabled = !payload || !available;
    byId("review-status").textContent = !available ? "Deze uitvoering heeft geen zoekresultaten om te beoordelen." : payload ?
      "Opgeslagen in deze browser · " + verdicts[payload.verdict] : "Nog niet beoordeeld. Kies eerst een oordeel.";
    byId("clear-review").disabled = !payload;
    const exports = exportedReviews();
    byId("export-reviews").disabled = !exports.length;
    byId("export-reviews").textContent = "Exporteer beoordelingen (" + exports.filter((value) => value.status === "completed").length + ")";
    byId("storage-notice").hidden = state.storageAvailable;
    byId("storage-notice").textContent = "Je browser bewaart de beoordelingen niet. Exporteer ze voordat je de pagina sluit.";
  }
  function renderDetail() {
    const row = rowById.get(state.selected);
    byId("result-detail").hidden = !row;
    if (!row) return;
    const status = grade(row);
    byId("detail-heading").textContent = methodName(row) + " · " + profileName(row);
    byId("detail-status").className = "badge " + status.style;
    byId("detail-status").textContent = status.text;
    byId("detail-caption").textContent = row.record.case_id + " · " + (row.record.execution_kind === "fixture" ? "Oefenrun, geen prestatiemeting" : "Echte lokale uitvoering") +
      " · dezelfde opgeslagen resultaten bij elke beoordeling";
    byId("result-error").hidden = !row.record.error;
    byId("result-error").textContent = row.record.error || "";
    const measurements = byId("measurements");
    measurements.replaceChildren();
    [["Lokaal zoeken", timing(row)], ["Opgehaald", (row.record.outcome?.hits.length || 0) + " passages"],
      ["Contextschatting", row.record.outcome ? number(row.record.outcome.estimated_context_tokens, 0) + " tokens" : "N.v.t."]]
      .forEach(([label, value]) => {
        const item = element("div", "measurement");
        item.append(element("span", "", label), element("strong", "", value)); measurements.append(item);
      });
    renderFacts(row); renderSources(row); renderTechnical(row); renderManual(row);
  }
  function renderOverview() {
    const host = byId("all-questions-summary");
    host.replaceChildren();
    const visible = currentRows(true), groups = new Map(), type = byId("assessment-select").value;
    byId("overview-help").textContent = (type === "manual" ?
      "Jouw expliciete oordelen, met onbeoordeelde uitvoeringen apart. " : type === "ai" ?
      "AI-beoordeling is gepauzeerd; er zijn geen AI-oordelen berekend. " :
      "Gemiddelde bewijsdekking per beoordeelbare uitvoering, met de noemer erbij. ") +
      "Zoektijd is de mediaan van alleen lokaal zoeken; voorbereiding staat apart.";
    visible.forEach((row) => {
      const id = row.record.config_id;
      if (!groups.has(id)) groups.set(id, []);
      groups.get(id).push(row);
    });
    if (!visible.length) { host.append(element("p", "help", "Geen uitvoeringen voor deze selectie.")); return; }
    const scroll = element("div", "summary-scroll"), table = element("table", "summary-table"), head = element("thead"), tr = element("tr");
    ["Combinatie", "Uitvoeringen", type === "manual" ? "Jouw beoordelingen" : type === "ai" ? "AI-beoordeling" : "Bewijsdekking", "Zoektijd · mediaan"]
      .forEach((title) => tr.append(element("th", "", title)));
    head.append(tr); table.append(head);
    const body = element("tbody");
    groups.forEach((group) => {
      const row = group[0], tr = element("tr"), th = element("th", "", methodName(row) + " · " + profileName(row));
      th.scope = "row"; tr.append(th, element("td", "", group.length));
      const coverage = group.map((item) => metric(item, "evidence_coverage")?.value).filter((value) => value !== null && value !== undefined);
      const times = group.filter((item) => item.record.execution_kind === "local_measured")
        .map((item) => item.record.timings.retrieval_ms).filter((value) => value !== null).sort((a, b) => a - b);
      const middle = Math.floor(times.length / 2);
      const median = !times.length ? null : times.length % 2 ? times[middle] : (times[middle - 1] + times[middle]) / 2;
      let quality = coverage.length ? number(100 * coverage.reduce((sum, value) => sum + value, 0) / coverage.length, 1) + "% · " + coverage.length + "/" + group.length + " beoordeelbaar" : "N.v.t. · 0/" + group.length;
      if (type === "ai") quality = "Gepauzeerd";
      if (type === "manual") {
        const counts = { good: 0, bad: 0, review_needed: 0, unreviewed: 0, unavailable: 0 };
        group.forEach((item) => {
          if (item.record.execution_status !== "completed") counts.unavailable++;
          else counts[manual(item)?.manual?.verdict || "unreviewed"]++;
        });
        quality = counts.good + " goed · " + counts.bad + " onvoldoende · " + counts.review_needed + " nader bekijken · " + counts.unreviewed + " onbeoordeeld" +
          (counts.unavailable ? " · " + counts.unavailable + " n.v.t." : "");
      }
      tr.append(element("td", "", quality),
        element("td", "", milliseconds(median)));
      body.append(tr);
    });
    table.append(body); scroll.append(table); host.append(scroll);
  }
  function renderAll() {
    const visible = currentRows();
    if (!visible.some((row) => row.record.run_id === state.selected)) state.selected = visible[0]?.record.run_id || null;
    renderGrid(visible); renderDetail(); renderOverview();
  }
  const questions = new Map();
  rows.forEach((row) => questions.set(row.record.case_id, row.record.question));
  questions.forEach((question, id) => addOption(byId("question-select"), id, id + " · " + question));
  methodIds.forEach((id) => addOption(byId("method-select"), id, methods[id][0]));
  profileIds.forEach((id) => addOption(byId("profile-select"), id, profiles[id].join(" / ")));
  const settings = new Map();
  rows.forEach((row) => { if (!settings.has(settingsKey(row))) settings.set(settingsKey(row), settingsName(row)); });
  settings.forEach((name, id) => addOption(byId("settings-select"), id, name));
  ["question-select", "method-select", "profile-select", "assessment-select", "settings-select"]
    .forEach((id) => byId(id).addEventListener("change", renderAll));
  document.querySelectorAll("[data-verdict]").forEach((button) => button.addEventListener("click", () => {
    const row = rowById.get(state.selected);
    if (!row || row.record.execution_status !== "completed") return;
    state.localReviews.set(row.record.run_id, newReview(row, button.dataset.verdict, manual(row)?.manual?.notes || ""));
    persistReviews(); renderGrid(currentRows()); renderDetail(); renderOverview();
  }));
  byId("review-notes").addEventListener("input", () => {
    const row = rowById.get(state.selected), review = row && manual(row);
    if (!review?.manual) return;
    state.localReviews.set(row.record.run_id, newReview(row, review.manual.verdict, byId("review-notes").value));
    persistReviews();
  });
  byId("clear-review").addEventListener("click", () => {
    const row = rowById.get(state.selected);
    if (!row) return;
    state.localReviews.set(row.record.run_id, newReview(row, null, ""));
    persistReviews(); renderGrid(currentRows()); renderDetail(); renderOverview();
  });
  byId("export-reviews").addEventListener("click", () => {
    const reviews = exportedReviews();
    if (!reviews.length) return;
    const blob = new Blob([reviews.map((review) => JSON.stringify(review)).join("\n") + "\n"], { type: "application/x-ndjson;charset=utf-8" });
    const url = URL.createObjectURL(blob), link = element("a");
    link.href = url; link.download = "retrieval-human-reviews-" + data.artifact_id.slice(0, 12) + ".jsonl";
    document.body.append(link); link.click(); link.remove(); URL.revokeObjectURL(url);
    byId("review-status").textContent = "Beoordelingen geëxporteerd. De zoekresultaten zijn niet gewijzigd.";
  });
  const measured = rows.filter((row) => row.record.execution_kind === "local_measured").length;
  byId("artifact-summary").textContent = rows.length + " uitvoeringen · " + questions.size + " vragen · " +
    new Set(rows.map((row) => row.record.config_id)).size + " combinaties · " + measured + " echte lokale metingen";
  byId("fixture-note").hidden = measured === rows.length;
  const sharedPreparations = data.shared_preparations || [];
  byId("shared-preparation-details").hidden = !sharedPreparations.length;
  sharedPreparations.forEach((batch) => {
    const list = element("dl", "definition-list"), summary = batch.summary;
    const host = byId("shared-preparation-summary");
    host.append(element("h3", "", "Batch " + batch.batch_id));
    appendDefinition(list, "Voorbereiding", summary.preparation_id);
    [["Bronnen lezen", summary.source_parse_ms], ["Modelidentiteit controleren", summary.model_identity_ms],
      ["Model laden", summary.model_load_ms], ["Documentembeddings", summary.document_inference_ms],
      ["Vraagembeddings", summary.query_inference_ms]].forEach(([label, value]) => appendDefinition(list, label, milliseconds(value)));
    appendDefinition(list, "Nieuwe documentembeddings", summary.document_inputs_encoded);
    appendDefinition(list, "Nieuwe vraagembeddings", summary.query_inputs_encoded);
    appendDefinition(list, "Zoekpad", "Dezelfde Rust-zoekberekening na het laden van de gekozen opslag",
      "SQLite is hier opslag en cache; dit is geen aparte SQLite FTS- of ANN-zoekmethode.");
    host.append(list);
  });
  loadReviews(); renderAll();
})();
