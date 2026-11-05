# Vergelijk zoeken over meerdere stappen

Deze proef gebruikt voor tien vastgelegde vragen steeds dezelfde Codex-assistent met vier zoekmethodes. Alleen de zoekmethode verandert. Elke zoekactie geeft maximaal drie bronstukken terug; iedere sessie mag maximaal vier zoekpogingen doen en duurt maximaal 120 seconden. Een ongeldige of mislukte zoekpoging telt mee.

De assistent krijgt de gebruikersvraag en de gevonden teksten. Hij bepaalt zelf of informatie ontbreekt en welke vervolgvraag nodig is. De beschermde bronverwachtingen worden uitsluitend achteraf geladen door `agent-assess` en `agent-report`; zij sturen de zoeklus niet.

## Eerst de planning bekijken

Dit maakt geen modelaanroepen:

```sh
cargo run --locked --release -- lab agent-plan
cargo run --locked --release -- lab agent-plan --cases T02,P06
```

De volledige planning bevat 40 sessies: tien vragen maal vier zoekmethodes. De kleine proef bevat acht sessies. De methodevolgorde wordt per vraag geroteerd; het is geen algemene snelheidsbenchmark met herhaalmetingen.

## Echte sessies uitvoeren

Gebruik een aangemelde, passende Codex-runtime en de bestaande modelcache. Kies één beschikbaar model via `COA_CODEX_MODEL` als je de standaard niet wilt gebruiken. Wijzig model, programma of brondata niet tussen de kleine en de resterende proef: de runtimeconfiguratie bevat ook de hashes van de gebruikte programma’s.

Bouw eenmaal en gebruik voor alle sessies exact dezelfde executable. Kies nieuwe uitvoerbestanden en één gezamenlijk ledger. De limiet van 40 generaties telt alle pogingen, ook mislukkingen; AI-beoordelaars worden niet aangeroepen.

```sh
cargo build --locked --release
cp target/release/meedoen-retrieval /tmp/coa-agentic-fixed

/tmp/coa-agentic-fixed lab agent-run --live \
  --cases T02,P06 \
  --output /tmp/coa-agentic-pilot.jsonl \
  --ledger /tmp/coa-agentic-budget.json \
  --max-generations 40 --max-seconds 4800 \
  --model-cache .model-cache
```

Controleer de acht opgeslagen pogingen voordat je verdergaat. Als code, brondata, model en limieten ongewijzigd zijn gebleven, tellen deze acht direct mee in de uiteindelijke 40. Voer ze niet opnieuw uit om een gunstiger resultaat te kiezen.

```sh
/tmp/coa-agentic-fixed lab agent-run --live \
  --cases P01,P07,P09,T01,IND26-05,IND26-03,IND26-07,IND26-09 \
  --output /tmp/coa-agentic-rest.jsonl \
  --ledger /tmp/coa-agentic-budget.json \
  --max-generations 40 --max-seconds 4800 \
  --model-cache .model-cache
```

Een technische fout stopt de batch. Er zijn geen verborgen herhalingen of automatische budgetvernieuwingen. Bekijk eerst de oorspronkelijke fout en gegevens; verander een ledger niet om eerdere pogingen te verbergen.

## Opgeslagen bronnen beoordelen en rapporteren

Deze opdrachten starten geen nieuwe Codex-sessies, zoekacties of AI-beoordelaars. Beide invoerbestanden blijven ongewijzigd.

```sh
/tmp/coa-agentic-fixed lab agent-assess \
  --input /tmp/coa-agentic-pilot.jsonl,/tmp/coa-agentic-rest.jsonl \
  --output /tmp/coa-agentic-assessments.jsonl

/tmp/coa-agentic-fixed lab agent-report \
  --input /tmp/coa-agentic-pilot.jsonl,/tmp/coa-agentic-rest.jsonl \
  --assessments /tmp/coa-agentic-assessments.jsonl \
  --output /tmp/coa-agentic-report.json
```

De beoordeling controleert de vraag, dataset, vaste instellingen, oorspronkelijke uitvoerbytes, runtimeconfiguratie en MCP-trace. Daarna telt zij uitsluitend de werkelijk aan het model aangeboden bronstukken over alle zoekacties samen. Dubbelen tellen eenmaal. Geldige alternatieven blijven geldig: voor de natuurvraag is het tijdstip nodig plus één van de twee geaccepteerde natuurstukken.

Het rapport bevat het werkelijke totale aantal zoekpogingen en de gemeten volledige sessieduur. De eerste stap waarop alle verwachte bronnen aanwezig waren wordt afzonderlijk bewaard: als het systeem daarna nog doorzoekt, verdwijnt dat extra werk niet uit het hoofdcijfer.

Een technisch mislukte sessie krijgt geen inhoudelijke succesbeoordeling. Ontbreekt het volledige runtimeverslag, dan blijft het aantal zoekpogingen onbekend; het aantal succesvolle sidecarregels is geen vervanging daarvoor. Een afgewezen verkeerde-modelstart blijft als fout zichtbaar met de waargenomen en verwachte runtime-identiteit.

Bronvolledigheid is geen beoordeling van de juistheid van het geschreven antwoord. De vrije AI-beoordelaar blijft uit en menselijke beoordelingen worden niet door agents ingevuld.
