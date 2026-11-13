(() => {
  const el = (id) => document.getElementById(id);
  const { t, current: language, activity, heading } = window.CoaLanguage;
  let session = "", source = { sources: [] }, currentRun = null, busy = false, pollVersion = 0, status = null;
  const errorKeys = {
    "Testsessie bestaat niet meer; start een nieuwe sessie.": "sessionExpired",
    "Testsessie bestaat niet meer.": "sessionExpired",
    "Start eerst een testsessie.": "sessionExpired",
    "Er draait al een vraag in deze sessie. Wacht tot die klaar is.": "runBusy",
    "Er worden al tien vragen verwerkt. Probeer het straks opnieuw.": "capacityFull",
    "De demo is druk. Probeer het straks opnieuw.": "capacityFull",
    "Please enter a question of 1 to 1000 characters.": "questionInvalid",
    "Lokale opslag niet beschikbaar; herstart de demo.": "storageUnavailable",
  };
  const errorCodes = {
    capacity: "capacityFull",
    rate_limit: "rateLimited",
    session_cooldown: "sessionCooldown",
    session_limit: "sessionLimited",
  };
  function requestError(key) { return Object.assign(new Error(key), { code: key }); }
  async function api(path, body) {
    let response;
    try {
      response = await fetch(path, {
        method: body ? "POST" : "GET",
        headers: { "Content-Type": "application/json", "X-Test-Session": session },
        ...(body ? { body: JSON.stringify(body) } : {}),
      });
    } catch { throw requestError("networkFailed"); }
    let data;
    try { data = await response.json(); } catch { throw requestError("requestFailed"); }
    if (!response.ok) throw requestError(errorCodes[data.code] || errorKeys[data.error] || "requestFailed");
    return data;
  }
  function setBusy(value) {
    busy = value;
    document.querySelectorAll(".run-control").forEach(item => { item.disabled = value; });
  }
  function renderStatus() {
    const parts = [];
    if (source.model_configuration?.offline_mock) parts.push(t("offlineTest"));
    if (status) parts.push(t(status.key, status.reason ? { reason: t(status.reason) } : {}));
    el("status").textContent = parts.join(" ");
    el("status").className = status?.error ? "error" : "";
  }
  function message(key, error = false, reason) {
    status = key ? { key, error, reason } : null;
    renderStatus();
  }
  function node(tag, text, className) {
    const item = document.createElement(tag);
    if (text !== undefined) item.textContent = text;
    if (className) item.className = className;
    return item;
  }
  function sourceTitle(id) {
    const doc = source.sources.find(doc => doc.id === id);
    return activity(id, doc?.title || id);
  }
  function sourceDate(date) { return t("sourceDate", { date }); }
  function failureKey(error) {
    if (error?.startsWith("tool:")) return "toolFailure";
    if (error?.startsWith("output:")) return "outputFailure";
    return "runtimeFailure";
  }
  function quotedPassage(text, quotes) {
    const block = node("blockquote");
    block.lang = "nl";
    const ranges = quotes.map(quote => ({ start: text.indexOf(quote), end: text.indexOf(quote) + quote.length }))
      .filter(range => range.start >= 0).sort((a, b) => a.start - b.start);
    let cursor = 0;
    for (const range of ranges) {
      if (range.start < cursor) continue;
      block.append(document.createTextNode(text.slice(cursor, range.start)), node("mark", text.slice(range.start, range.end)));
      cursor = range.end;
    }
    block.append(document.createTextNode(text.slice(cursor)));
    return block;
  }
  function showRun(run) {
    currentRun = run;
    el("result").hidden = false;
    el("result-title").textContent = t(run.status === "failed" ? "failedTitle" : "answer");
    el("answer").textContent = run.status === "failed" ? t(failureKey(run.error)) : run.result?.answer?.text || t("pendingAnswer");
    el("answer").lang = run.result?.answer?.text ? run.context?.answer_language || "nl" : language();
    el("answer").classList.toggle("error", run.status === "failed");
    const method = run.result?.retrieval_method;
    const methodKey = { dense: "methodDense", bm25: "methodBm25" }[method];
    el("run-method").hidden = !methodKey;
    el("run-method").textContent = methodKey ? t("runMethod", { method: t(methodKey) }) : "";
    const citations = run.result?.answer?.citations || [];
    el("tools").replaceChildren();
    (run.result?.tool_calls || []).forEach((call, index) => {
      const item = node("article", undefined, "search-action");
      item.append(node("h3", t("searchAction", { number: index + 1 }), "search-action-heading"),
        node("p", call.query, "search-query"));
      (call.result?.hits || []).forEach(hit => {
        const evidence = node("section", undefined, "retrieved");
        evidence.append(
          node("h4", sourceTitle(hit.section.source_id) + " · " + heading(hit.section.heading)),
          quotedPassage(hit.section.text, citations.filter(c => c.source_id === hit.section.source_id && c.source_date === hit.section.source_date && c.section_id === hit.section.id).map(c => c.quote)),
          node("small", t("published", { date: hit.section.source_date })),
        );
        item.append(evidence);
      });
      if (call.error || call.status === "failed") item.append(node("p", t("searchFailed"), "error"));
      else if (call.status !== "completed") item.append(node("p", t("searchPending"), "muted"));
      el("tools").append(item);
    });
    el("tool-details").hidden = !run.result?.tool_calls?.length;
  }
  function renderSourceLabels() {
    document.querySelectorAll(".source-document").forEach(item => {
      const doc = source.sources.find(doc => doc.id === item.dataset.source);
      item.querySelector("summary").textContent = sourceTitle(doc.id);
      item.querySelector("small").textContent = sourceDate(doc.date);
      item.querySelectorAll(".source-section h3").forEach((label, index) => {
        const section = doc.sections[index];
        label.textContent = section.id + " · " + heading(section.heading);
      });
    });
  }
  function renderLanguage() {
    el("question").placeholder = t("questionPlaceholder");
    el("result").setAttribute("aria-label", t("resultLabel"));
    renderSourceLabels();
    renderStatus();
    if (currentRun) showRun(currentRun);
  }
  window.addEventListener("languagechange", renderLanguage);
  async function watch(run) {
    const version = ++pollVersion;
    setBusy(true);
    showRun(run);
    message("searching");
    try {
      while (run.status === "pending") {
        await new Promise(resolve => setTimeout(resolve, 750));
        run = await api("/api/runs/" + run.id);
        if (version !== pollVersion) return;
      }
      showRun(run);
      message(run.status === "failed" ? "failedStatus" : null, run.status === "failed");
    } catch (error) { message(error.code || "requestFailed", true); }
    finally { if (version === pollVersion) setBusy(false); }
  }
  async function begin() {
    if (busy) return;
    if (!source.runtime_ready) { message("runtimeUnavailable", true); return; }
    setBusy(true);
    message("starting");
    try {
      const run = await api("/api/runs", { question: el("question").value.trim(), top_k: 3, answer_language: language(), retrieval_method: el("retrieval-method").value });
      await watch(run);
    } catch (error) {
      setBusy(false);
      message(error.code || "requestFailed", true);
    }
  }
  el("form").addEventListener("submit", event => { event.preventDefault(); begin(); });
  el("question").addEventListener("input", () => {
    el("result").hidden = true;
    currentRun = null;
    message(null);
  });
  async function createSession() {
    const data = await api("/api/session", { seen_question_ids: [] });
    session = data.session_id;
    try { sessionStorage.setItem("coa-test-session", session); } catch {}
    return data.summary;
  }
  renderLanguage();
  (async () => {
    setBusy(true);
    try {
      source = await api("/api/source");
      source.sources.forEach(doc => {
        const document = node("details", undefined, "source-document");
        document.name = "activity";
        document.dataset.source = doc.id;
        document.append(node("summary", sourceTitle(doc.id)), node("small", sourceDate(doc.date)));
        doc.sections.forEach(section => {
          const item = node("section", undefined, "source-section");
          item.id = "source-" + doc.id + "-" + section.id;
          const text = node("p", section.text);
          text.lang = "nl";
          item.append(node("h3", section.id + " · " + heading(section.heading)), text);
          document.append(item);
        });
        el("source-sections").append(document);
      });
      try { session = sessionStorage.getItem("coa-test-session") || ""; } catch { session = ""; }
      let summary;
      try {
        if (!session) throw new Error();
        summary = await api("/api/session");
      } catch { summary = await createSession(); }
      setBusy(false);
      el("question").focus();
      const pending = summary.runs.find(run => run.status === "pending");
      if (pending) {
        el("question").value = pending.question;
        await watch(pending);
      } else if (!source.runtime_ready) message("runtimeUnavailable", true);
    } catch (error) {
      message("loadFailed", true, error.code || "requestFailed");
      setBusy(false);
    }
  })();
})();
