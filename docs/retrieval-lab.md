# Retrievalvarianten lokaal vergelijken

Het lab gebruikt dezelfde vijf fictieve activiteiten, dertig oorspronkelijke passages en achttien bestaande ontwikkel-/regressievragen. De drie hoofdkeuzes zijn **zoekmethode, opslag/structuur en beoordelingstype**.

| Keuze | Varianten |
| --- | --- |
| Zoekmethode | `dense`, `bm25`, `hybrid_rrf`, `hybrid_weighted` |
| Opslag/structuur | `documents_sections`, `documents_activities`, `sqlite_sections`, `sqlite_activities` |
| Beoordeling | `deterministic`, `manual`, `ai` (gepauzeerd) |

De volledige hoofdmatrix heeft **16 varianten × 18 vragen = 288 zoekuitvoeringen**. Verschillende beoordelingen gebruiken daarna dezelfde opgeslagen resultaten. Het lab genereert geen antwoorden en doet geen Codex- of AI-beoordelaarsaanroepen.

## Uitvoeren

Voer dit uit vanuit de projectmap met de bestaande `.model-cache`. Zonder modelbestanden kan de eerste vectoruitvoering ongeveer 235 MB downloaden. Keyword-only zoeken heeft geen embeddingmodel nodig.

```sh
LAB_RUN_DIR="$(mktemp -d /tmp/coa-retrieval-lab.XXXXXX)"
export CARGO_TARGET_DIR="$PWD/target/retrieval-lab"

cargo run --locked --features sqlite -- lab plan
cargo run --locked --features sqlite -- lab run \
  --model-cache "$PWD/.model-cache" \
  --sqlite-dir "$LAB_RUN_DIR/cache" \
  --output "$LAB_RUN_DIR/retrieval.jsonl"
cargo run --locked --features sqlite -- lab assess \
  --input "$LAB_RUN_DIR/retrieval.jsonl" \
  --types deterministic,manual,ai \
  --output "$LAB_RUN_DIR/assessments.jsonl"
```

De SQLite-feature is optioneel. `--no-default-features` bouwt een zelfstandige documentvariant; geplande SQLite-rijen blijven zichtbaar als niet ondersteund. Bestaande resultaatbestanden worden nooit overschreven. Gebruik voor een nieuwe uitvoering een nieuw resultaatbestand; dezelfde cachemap mag worden hergebruikt.

Een kleine proef zonder embeddingmodel:

```sh
cargo run --locked --no-default-features -- lab run \
  --methods bm25 --profiles documents_sections --cases T02 \
  --output "$LAB_RUN_DIR/t02-keyword.jsonl"
```

## Resultaten bekijken en zelf beoordelen

Maak van opgeslagen uitvoeringen een zelfstandig HTML-rapport:

    cargo run --locked --features sqlite -- lab report --input "$LAB_RUN_DIR/retrieval.jsonl" --assessments "$LAB_RUN_DIR/assessments.jsonl" --output "$LAB_RUN_DIR/report.html"
    open "$LAB_RUN_DIR/report.html"

**open** is voor macOS; op andere systemen kun je het bestand in je browser openen. Het rapport heeft geen server, externe scripts of internetverbinding nodig. De report-opdracht laadt geen embeddingmodel en doet geen nieuwe zoek- of modelaanroepen. Zonder **--assessments** vult zij de actuele deterministische metingen, lege handmatige beoordelingen en gepauzeerde AI-records offline aan.

1. Kies een vraag, zoekmethode, opslag-/structuurprofiel en beoordeling.
2. Open een resultaat. Links staan de verwachte feiten; rechts de werkelijk opgehaalde passages met bron en datum.
3. Bekijk wat ontbreekt en open desgewenst de metingen en instellingen.
4. Kies **Zelf beoordelen** om een oordeel en optionele notitie vast te leggen. Je browser bewaart dit lokaal; exporteer de beoordelingen voor een los JSONL-bestand.

Importeer zo'n export in een nieuw rapport:

    cargo run --locked --features sqlite -- lab report --input "$LAB_RUN_DIR/retrieval.jsonl" --assessments /pad/naar/beoordelingen.jsonl --output "$LAB_RUN_DIR/report-met-oordelen.html"

Het lab behoudt de gegeven oordelen en vult ontbrekende beoordelingstypen aan. Een verkeerd gekoppelde of verouderde beoordeling wordt geweigerd; zoeken wordt niet herhaald. Browseropslag die niet beschikbaar is, wordt zichtbaar gemeld. Niet-uitgevoerde combinaties tonen geen verzonnen nulscore.

Lokale tijden bewijzen geen algemene snelheidswinst. Het retrieval-buildprofiel staat niet in de records: gebruik voor prestatieonderzoek consequent **--release** en bewaar ook het commando, de binaryhash en de omgeving. De pagina telt gedeelde voorbereiding eenmaal per batch en houdt deze los van de lokale zoektijden.

## Wat wordt beoordeeld?

Deterministische controles controleren bronidentiteit, ongewijzigde passages, configuratie, herleidbare zoekresultaten en contextlimieten. De **bewijsdekking** meet welke verwachte feiten volledig ondersteund worden. Relevantie-/rangschikkingsscores zijn alleen beschikbaar bij volledige relevantielabels. Geen enkele score is automatisch een algemeen kwaliteitsoordeel.

Voor T02 (dinsdag, binnen, tijd en adres) zijn de verwachte bronpassages `ACT-FIETS-002:S2` en `S3`. Alleen S2 ondersteunt 0/2 complete bewijsdoelen; alleen S3 ondersteunt 1/2; samen ondersteunen ze 2/2. S2 bevat dus wel nuttige informatie, ook wanneer nog geen volledig bewijsdoel af is. De brondatum is 18 oktober 2026. Dit zijn verwachtingen, geen opgelegde zoekresultaten.

Handmatige beoordelingen beginnen leeg. Een mens kan later een oordeel en notitie invullen. AI-beoordelingen blijven gepauzeerd. Zonder gegenereerd antwoord zijn antwoordjuistheid en antwoordcitaten **niet van toepassing**. Een onbekend feit is geen positief zoekdoel en krijgt geen verzonnen perfecte score.

Beoordelingen zijn gekoppeld aan de SHA-256 van de exacte opgeslagen JSONL-regel. Bij opnieuw beoordelen wordt niet opnieuw gezocht. Een beoordeling van andere of gewijzigde resultaatbytes wordt geweigerd.

Assessor `retrieval-assessment-v2` controleert ook alle niet-geselecteerde kandidaten en weggelaten verwijzingen. Bewijsdekking van ongewijzigde bronpassages blijft een afzonderlijke meting; een ongeldige trace krijgt geen geldige rangschikkingsscore. Maak bij een oudere assessorversie een nieuw beoordelingsbestand van dezelfde opgeslagen zoekresultaten. De zoekresultaten hoeven daarvoor niet opnieuw te worden uitgevoerd.

## Verdieping en eerlijke metingen

Geavanceerde opties zijn `--top-k 3|6`, `--candidate-k`, `--context-limit`, `--selection sections|activity_expansion`, `--mmr on|off`, `--apply-filters on|off`, `--rrf-k`, `--vector-weight` en `--mmr-lambda`. `--query-filters` accepteert expliciete JSON-criteria, bijvoorbeeld `'{"weekdays":["tuesday"],"environment":"indoor"}'`. Deze gelden voor de geselecteerde vragen; het lab leidt geen criteria uit verwachte antwoorden af. Onbekende bronvelden blijven onbekend.

Activiteituitbreiding is een afzonderlijke contextkeuze: oorspronkelijke passages blijven heel en worden alleen toegevoegd zolang het gedeelde contextbudget dat toelaat. Activiteitrecords vormen wel een echt ander gegevensmodel, met brongebonden velden en gekoppelde passages. Opslag, groepering, filters en zoekmethode worden afzonderlijk vastgelegd.

Alle opslagroutes laden dezelfde canonieke passages en exacte f32-embeddings in dezelfde Rust-scorer. SQLite gebruikt hier geen andere native vectorzoekmethode of FTS-ranker. Opslag veranderen hoort daardoor op zichzelf geen betere relevantie te geven.

`retrieval_ms` meet de lokale zoekberekening, inclusief selectie en controles, zonder modelinitialisatie, embeddings of opslagvoorbereiding. Model-/querywerk en cachehergebruik worden gedeeld en apart vastgelegd in `shared_preparation`. Tel kopieën met dezelfde identiteit niet bij elkaar op. Voorbereiding per profiel hangt af van uitvoeringsvolgorde en cachestatus; gebruik die niet als ongecontroleerd bewijs dat een opslagvariant sneller is. Geschatte contexttokens zijn geen werkelijk providerverbruik.

Een verse cachemap geeft een nieuwe lokale index/cache; modelbestanden blijven afzonderlijk in `.model-cache`. Bewaar koud/warm als aparte uitvoeringen en vermeld de toestand. De bestaande vragen zijn al eerder bekeken en leveren geen nieuwe ongeziene benchmark op.
