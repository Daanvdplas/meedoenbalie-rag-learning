# Meedoenbalie: één bron, vijf vragen

Een kleine Rust-leerproef met één verzonnen activiteit, echte lokale embeddings en Codex. Je beoordeelt zelf het antwoord en de gebruikte bronpassages. Dit is een eigen prototype, geen intern COA-plan.

## Starten

Benodigd: Rust 1.94.1 en een aangemelde Codex-client. De demo zoekt lokaal naar de bij de Codex-app horende runtime. Een losse CLI moet een bijpassende `codex-code-mode-host` hebben. Zo nodig kun je het juiste uitvoerbare bestand instellen met `COA_CODEX_BIN`; bestaande aanmeldgegevens blijven buiten de runverslagen.

```sh
cargo run --locked
```

Open **http://127.0.0.1:8766/**. De eerste start downloadt ongeveer 235 MB aan modelbestanden en aanvullende bestanden naar `.model-cache/`. Voor embeddings is geen API-sleutel nodig; Codex gebruikt je bestaande aanmelding. De server start nadat het lokale model is geladen.

Een andere lokale poort kan met `COA_PORT=8767 cargo run --locked`. `COA_CODEX_MODEL` kan het expliciet gekozen antwoordmodel veranderen; de gebruikte versie en instellingen worden per run vastgelegd.

## Zelf testen

1. Lees het brondocument en kies Q1–Q5, of schrijf een eigen vraag.
2. Laat Codex zoeken en antwoorden. Bekijk de exacte citaten, brondatum en gebruikte zoekvragen/passages.
3. Kies **Goed**, **Aandacht nodig** of **Weet ik niet**. Pas daarna zijn de voorbereide criteria beschikbaar. Een eigen vraag telt niet mee bij de vijf vragen.
4. Vergelijk verwachte en gevonden bronsecties. Het eerste oordeel blijft staan; herhaalde beoordelingen en uitvoeringen zijn als voorkennis gemarkeerd.
5. Bekijk eerdere runs, download het testverslag of voer een vraag opnieuw uit. Een nieuwe generatie krijgt een eigen run. Gewijzigde of ontbrekende bron/runtime-instellingen worden niet stilzwijgend hergebruikt.

Een echt Codex-antwoord duurt in de lokale proef vaak ongeveer 45–50 seconden; de pagina toont ondertussen de voortgang en de bron blijft leesbaar. Er kan één modeluitvoering tegelijk lopen. Fouten blijven zichtbaar en tellen niet als geslaagde tests. Runs en oordelen staan uitsluitend in begrensd servergeheugen en verdwijnen bij herstart. De browser onthoudt welke criteria je al zag, ook na een nieuwe testsessie; voor onafhankelijke beoordeling gebruik je een apart browserprofiel/context.

## Rust-zoekfunctie en evaluatie

```sh
cargo run --locked -- search 'Waar en wanneer is het tuinieren?'
cargo run --locked -- ask 'Zijn er nog plekken vrij?'
cargo run --locked -- eval > /tmp/meedoen-eval.json
cargo test --locked
```

`fixtures/document.md` is de enige geïndexeerde bron. Zes secties krijgen embeddings van het lokale, gekwantiseerde meertalige MiniLM-model via fastembed 7.1.2. CLI en MCP gebruiken dezelfde Rust-zoekfunctie. Standaard worden drie secties gerangschikt; `--top-k 1..6` is instelbaar. Overeenkomst is geen zekerheid en er is geen gekalibreerde grens voor “geen match”.

`fixtures/questions.json` bevat vijf vragen; `fixtures/expectations.json` is uitsluitend voor evaluatie en menselijke beoordeling. Verwachte antwoorden gaan niet naar de index, zoekresultaten of Codex. Codex krijgt alleen de vraag en daadwerkelijk opgehaalde passages via de beperkte MCP-tool.

Het [vastgelegde zoekexperiment](reports/one-document-eval.json) vergelijkt embeddings met een simpele woordoverlap-baseline. Gemiddelde bronsectiedekking op vier vragen met bronlabels is 0,875 versus 0,75; dit is geen antwoordnauwkeurigheid. Q1 mist bij de eerste drie zoekhits locatie S3. Q5 heeft geen relevante bronsectie. Q4 vindt alleen historische beschikbaarheid; actuele plekken blijven onbekend. Codex kan zelf aanvullende zoekvragen stellen. Vijf vragen vormen geen bewijs voor algemene kwaliteit.

CI controleert formattering, clippy, deterministische tests en build zonder modelaanroepen. De echte modelcontrole staat apart: `cargo test --locked live_report_retains -- --ignored`. [Besluiten](decisions.md) beschrijft zoekinstellingen, brondata en beperkingen van reproduceerbaarheid.

## Grenzen en werkwijze

Alle gegevens zijn verzonnen. Suggesties behouden bron en datum; ontbrekende of oude feiten blijven onbekend. Deelname is vrijwillig en een medewerker bespreekt opties met de bewoner. Geen dossierwijzigingen, berichten of aanmeldingen. De demo luistert alleen op localhost.

Alle codewijzigingen gaan via branches en pull requests, met onafhankelijke beoordeling. Niet rechtstreeks naar `main` committen; alleen mergen met toestemming van de gebruiker.

Bron: [fastembed 7.1.2](https://github.com/anush008/fastembed-rs/releases/tag/v7.1.2), gepubliceerd **19 oktober 2026**. Zie verder [projectinstructies](AGENTS.md), [use case](user_journey.md) en [2026-oriëntatie](agent-harness-landscape.md). Het langetermijndoel blijft een kleine, uitlegbare agent harness; deze proef houdt bewust één bron en vijf vragen.
