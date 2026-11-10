(() => {
  const messages = {
    navigation: ["Hoofdnavigatie", "Main navigation"],
    language: ["Taal", "Language"],
    chooseLanguage: ["Taal kiezen: {language}", "Choose language: {language}"],
    brand: ["COA Assistent", "COA Assistant"],
    demo: ["Demo", "Demo"],
    technical: ["Hoe het werkt", "How it works"],
    code: ["Bekijk de code", "View the code"],
    projectNotice: [
      "Eigen leerproject, geen intern COA-plan. Alle gegevens zijn verzonnen. Meedoen is vrijwillig; een medewerker bespreekt opties met de bewoner. De demo wijzigt geen dossiers, verstuurt geen berichten en meldt niemand aan.",
      "An independent learning project, not an internal COA plan. All data is fictional. Participation is voluntary; a staff member discusses options with the resident. The demo does not change records, send messages or register anyone."
    ],
    projectShortNotice: [
      "Eigen leerproject, geïnspireerd door de COA-use case. Alle activiteiten zijn verzonnen.",
      "An independent learning project inspired by the COA use case. All activities are fictional."
    ],
    pageTitle: ["COA Assistent · Vind een activiteit die past", "COA Assistant · Find an activity that fits"],
    technicalPageTitle: ["Hoe het werkt · COA Assistent", "How it works · COA Assistant"],
    resultsPageTitle: ["Tests en resultaten · COA Assistent", "Tests and results · COA Assistant"],
    activities: ["Activiteiten", "Activities"],
    questionTitle: ["Vind een activiteit die past", "Find an activity that fits"],
    questionLabel: ["Stel de COA assistent een vraag", "Ask the COA assistant a question"],
    questionPlaceholder: ["Welke activiteiten zijn er op dinsdag?", "Which activities are there on Tuesday?"],
    retrievalMethod: ["Zoekmethode", "Search method"],
    methodDense: ["Op betekenis", "By meaning"],
    methodBm25: ["Op woorden", "By words"],
    runMethod: ["Zoekmethode: {method}", "Search method: {method}"],
    submit: ["Zoek een antwoord", "Find an answer"],
    resultLabel: ["Uitvoering en antwoord", "Run and answer"],
    answer: ["Het antwoord", "The answer"],
    failedTitle: ["De uitvoering is mislukt", "The run failed"],
    pendingAnswer: ["Codex is nog bezig…", "Codex is still working…"],
    results: ["Bekijk de resultaten", "View the results"],
    starting: ["Uitvoering starten…", "Starting the run…"],
    searching: ["Codex zoekt passages en maakt een antwoord. Dit duurt vaak ongeveer een minuut. Je kunt de bron blijven bekijken.", "Codex is finding passages and preparing an answer. This often takes about a minute. You can keep reading the sources."],
    failedStatus: ["Er is geen antwoord opgeslagen. Je kunt de vraag opnieuw indienen.", "No answer was saved. You can submit the question again."],
    toolFailure: ["De bronzoekfunctie is niet goed afgerond. Er is geen antwoord opgeslagen.", "The source search did not complete successfully. No answer was saved."],
    outputFailure: ["Het antwoord kon niet aan de gevonden bronpassages worden gekoppeld. Er is geen antwoord opgeslagen.", "The answer could not be linked to the retrieved source passages. No answer was saved."],
    runtimeFailure: ["Codex kon de vraag niet afronden. Controleer de aanmelding of probeer opnieuw.", "Codex could not complete the question. Check your sign-in or try again."],
    runtimeUnavailable: ["Codex is nog niet beschikbaar. Controleer de aanmelding en herstart de demo.", "Codex is not available yet. Check your sign-in and restart the demo."],
    requestFailed: ["Het verzoek is mislukt. Probeer opnieuw.", "The request failed. Please try again."],
    networkFailed: ["De lokale server is niet bereikbaar. Controleer of de demo nog draait en probeer opnieuw.", "The local server could not be reached. Check that the demo is running and try again."],
    loadFailed: ["De demo kon niet worden geladen: {reason}", "The demo could not be loaded: {reason}"],
    sessionFull: ["Je hebt in deze sessie 40 vragen gesteld. Open de demo in een nieuw tabblad om verder te gaan.", "You have asked 40 questions in this session. Open the demo in a new tab to continue."],
    sessionExpired: ["Deze sessie is verlopen. Vernieuw de pagina om verder te gaan.", "This session has expired. Refresh the page to continue."],
    sessionsFull: ["Het maximum van 16 lokale sessies is bereikt. Herstart de demo.", "The limit of 16 local sessions has been reached. Restart the demo."],
    runBusy: ["Er wordt al een vraag verwerkt. Wacht tot die klaar is.", "A question is already being processed. Wait for it to finish."],
    questionInvalid: ["Vul een vraag van 1 tot 1000 tekens in.", "Enter a question between 1 and 1000 characters."],
    storageUnavailable: ["Lokale opslag is niet beschikbaar. Herstart de demo.", "Local storage is not available. Restart the demo."],
    searchAction: ["Zoekactie {number}", "Search {number}"],
    searchFailed: ["Deze zoekactie is mislukt.", "This search failed."],
    searchPending: ["Deze zoekactie is nog niet afgerond.", "This search has not finished yet."],
    sourceDate: ["Originele bron (Nederlands) · brondatum {date}", "Original source (Dutch) · published {date}"],
    published: ["Brondatum {date}", "Published {date}"],
    gardening: ["Tuinieren", "Gardening"],
    bicycles: ["Fietsen opknappen", "Bicycle repair"],
    languageCafe: ["Taalcafé", "Language café"],
    cooking: ["Samen koken", "Cooking together"],
    walking: ["Wandelen", "Walking"],
    creative: ["Creatief atelier", "Creative workshop"],
    park: ["Parkonderhoud", "Park maintenance"],
    library: ["Buurtbibliotheek", "Community library"],
    music: ["Muziekgroep", "Music group"],
    digital: ["Digitale hulp", "Digital support"],
    what: ["Wat doen we?", "What do we do?"],
    when: ["Wanneer?", "When?"],
    where: ["Waar?", "Where?"],
    support: ["Taal en hulp", "Language and support"],
    bring: ["Wat neem je mee?", "What should you bring?"],
    costs: ["Kosten en materiaal", "Costs and materials"],
    availability: ["Beschikbaarheid", "Availability"],
  };
  const activityKeys = {
    "ACT-TUIN-001": "gardening", "ACT-FIETS-002": "bicycles", "ACT-TAAL-003": "languageCafe",
    "ACT-KOKEN-004": "cooking", "ACT-WANDEL-005": "walking", "ACT-MAAK-006": "creative",
    "ACT-PARK-007": "park", "ACT-BIEB-008": "library", "ACT-MUZIEK-009": "music", "ACT-DIGI-010": "digital",
  };
  const headingKeys = {
    "Wat doen we?": "what", "Wanneer?": "when", "Waar?": "where", "Taal en hulp": "support",
    "Wat neem je mee?": "bring", "Kosten en materiaal": "costs", "Beschikbaarheid": "availability",
  };
  let language = "nl";
  try { if (localStorage.getItem("coa-demo-language") === "en") language = "en"; } catch {}
  function t(key, values = {}) {
    const template = messages[key]?.[language === "en" ? 1 : 0] || key;
    return template.replace(/\{(\w+)\}/g, (_, name) => String(values[name] ?? ""));
  }
  function render() {
    document.documentElement.lang = language;
    document.querySelectorAll("[data-i18n]").forEach(item => { item.textContent = t(item.dataset.i18n); });
    document.querySelector(".site-nav")?.setAttribute("aria-label", t("navigation"));
    document.querySelector(".language-switch")?.setAttribute("aria-label", t("language"));
    document.querySelectorAll("[data-language]").forEach(button => button.setAttribute("aria-pressed", String(button.dataset.language === language)));
    const trigger = document.getElementById("language-trigger");
    if (trigger) {
      const label = t("chooseLanguage", { language: language === "nl" ? "Nederlands" : "English" });
      trigger.setAttribute("aria-label", label);
      trigger.title = label;
      document.getElementById("current-language-flag").textContent = language === "nl" ? "🇳🇱" : "🇬🇧";
      document.getElementById("language-options").setAttribute("aria-label", t("language"));
    }
    if (document.querySelector("#form")) document.title = t("pageTitle");
  }
  function select(next) {
    if (!["nl", "en"].includes(next) || next === language) return;
    language = next;
    try { localStorage.setItem("coa-demo-language", language); } catch {}
    render();
    window.dispatchEvent(new Event("languagechange"));
  }
  window.CoaLanguage = {
    t,
    current: () => language,
    activity: (id, fallback) => activityKeys[id] ? t(activityKeys[id]) : fallback,
    heading: (heading) => headingKeys[heading] ? t(headingKeys[heading]) : heading,
  };
  const picker = document.querySelector("details.language-switch");
  const trigger = document.getElementById("language-trigger");
  function closePicker(restoreFocus = false) {
    if (!picker) return;
    picker.open = false;
    trigger.setAttribute("aria-expanded", "false");
    if (restoreFocus) trigger.focus();
  }
  picker?.addEventListener("toggle", () => trigger.setAttribute("aria-expanded", String(picker.open)));
  document.querySelectorAll("[data-language]").forEach(button => button.addEventListener("click", () => {
    select(button.dataset.language);
    closePicker(true);
  }));
  document.addEventListener("keydown", event => {
    if (event.key === "Escape" && picker?.open) {
      event.preventDefault();
      closePicker(true);
    }
  });
  document.addEventListener("click", event => {
    if (picker?.open && !picker.contains(event.target)) closePicker();
  });
  render();
})();
