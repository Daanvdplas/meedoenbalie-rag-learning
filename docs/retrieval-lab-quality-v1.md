# Vastgelegde kwaliteitsvergelijking

`lab` houdt zoekresultaten, beschermde verwachtingen en beoordelingen apart. Een
opgeslagen zoekactie opnieuw beoordelen voert geen zoekactie of modelaanroep uit.
De vrije AI-beoordelaar blijft gepauzeerd. Menselijke beoordelingen starten leeg.

## Twee afzonderlijke vraagsets

- `--dataset legacy` (standaard): de bestaande achttien eerder bekeken vragen.
  Bestaande raw-schema-1 bestanden en v2-beoordelingen blijven leesbaar; hun
  vraag- en verwachtingenhash blijven ongewijzigd.
- `--dataset independent-20261103`: twaalf vóór de eerste meting vastgelegde
  vragen, door afzonderlijke agents geschreven en op de bronnen gecontroleerd.
  Dit zijn vragen over dezelfde vijf bekende documenten, geen menselijke ijking
  of bewijs van kwaliteit op nieuwe organisaties, documenten of vraagsoorten.
  Elf vragen hebben brononderbouwing als doel; één vraagt om verduidelijking en
  telt niet mee in retrievalnoemers.

De datasetkeuze is expliciet bij **plan, run, assess en report**. Een verkeerde
keuze wordt geweigerd voordat een beoordelings- of rapportbestand ontstaat.
Vragen, bronidentiteit, getypeerde gebruikersinput en verwachtingen krijgen elk
hun eigen binding. Geïmporteerde deterministische scores worden volledig opnieuw
berekend; alleen een overeenkomende hash is onvoldoende.

## Vooraf vastgelegde scenario’s

```sh
cargo run --locked -- lab plan --dataset independent-20261103 --protocol quality-v1
```

Dit plant **336 rijen**: twaalf vragen × 28 configuraties.

| Scenario | Profielen | Methoden | Selectie | Top-k | Filters |
| --- | --- | --- | --- | --- | --- |
| Baseline | alle vier | alle vier | secties | 3 | uit |
| Activiteitcontext | documenten/secties | alle vier | activiteituitbreiding | 3 | uit |
| Meer secties | documenten/secties | alle vier | secties | 6 | uit |
| Gebruikersfilters | documenten/secties | alle vier | activiteituitbreiding | 3 | aan |

Alle scenario’s houden candidate-k 30, contextlimiet 1200 en MMR uit. De zestien
baselineconfiguraties blijven apart van de twaalf extra configuraties. Top-k 3
betekent drie startscores bij activiteituitbreiding; complete, ongewijzigde
bronsecties kunnen daarna toegevoegd worden binnen dezelfde contextlimiet.

De legacy-set gebruikt met `quality-v1` **432 rijen**: achttien × 24 configuraties.
De filterbehandeling ontbreekt daar bewust: er zijn geen vooraf afzonderlijk
beoordeelde, getypeerde gebruikersfilters voor die vragen. `lab plan` zonder
protocol blijft de oorspronkelijke 288 baseline-rijen plannen.

De frozen set bevat een afzonderlijk `user-query-inputs.json`. Dit bestand bevat
uitsluitend de letterlijke gebruikersvragen en ondersteunde harde voorwaarden:
dagen, binnen/buiten en volledige beschikbaarheidsvensters. Het gebruikt geen
verwachte activiteiten, bronpassages, rollen of antwoorden. Een voorkeur voor
buiten wordt geen harde filter. Een eis zoals “rolstoeltoegankelijk”, betaald werk
of een exacte starttijd wordt niet verzonnen als een ondersteund veld. De huidige
roosters werken op minuten: “vóór 11:00” is daarom uiterlijk 10:59.

De onafhankelijk beoordeelde invoerbytes zijn afzonderlijk vastgelegd in
`INPUTS_FROZEN.json`. De runtime controleert dit goedgekeurde bestand, de exacte
invoerhash, de oorspronkelijke freeze en de vraaghashes vóór planning, zoeken of
beoordelen. Een geldig invoerschema met gewijzigde filterwaarden kan daardoor
niet dezelfde frozen dataset blijven heten.

Vijf vragen hebben ten minste één ondersteunde filter; zeven niet. Het rapport
onderscheidt filters uit, geen filters gevraagd, geen aangetoond effect en een
werkelijk kleinere kandidatenverzameling. Dat laatste betekent minder geschikte
kandidaten na filtering, **niet** automatisch minder uitgevoerde vectorscores.
Bij deze kleine Rust-ranker blijft het scoren van het corpus afzonderlijk gemeten.

## Uitvoeren en opnieuw beoordelen

Eerste neurale meting: uitsluitend na onafhankelijke review van de input-sidecar
én expliciete vrijgave van het frozen protocol. Dit commando plant geen Codex- of
judge-aanroepen. SQLite werkt alleen in een build met de optionele feature.

```sh
cargo run --locked --features sqlite -- lab run \
  --dataset independent-20261103 --protocol quality-v1 \
  --model-cache .model-cache --sqlite-dir /tmp/coa-quality-cache \
  --output /tmp/coa-quality-independent.jsonl

cargo run --locked --features sqlite -- lab assess \
  --dataset independent-20261103 --input /tmp/coa-quality-independent.jsonl \
  --output /tmp/coa-quality-independent-assessments.jsonl

cargo run --locked --features sqlite -- lab report \
  --dataset independent-20261103 --input /tmp/coa-quality-independent.jsonl \
  --assessments /tmp/coa-quality-independent-assessments.jsonl \
  --output /tmp/coa-quality-independent.html
```

Gebruik steeds nieuwe uitvoerbestanden. Oude bestanden worden niet overschreven.
Een batch deelt bron-/modelvoorbereiding en query-embeddings tussen configuraties;
het rapport bewaart die gezamenlijke tellers één keer. Tel herhaalde
voorbereidingsvelden niet op. Het kan zonder SQLite: die geplande configuraties
krijgen dan zichtbaar `unsupported` in plaats van een verzonnen meting.

## Wat het rapport wel en niet bewijst

De voornaamste retrievalmaat is **alle verwachte brononderbouwing aanwezig**,
met gedeeltelijke dekking daarnaast. Een eerste relevante passage of een hoge
cosinusscore is geen compleet antwoord. Onbekende voorwaarden, uitsluitingsbewijs
en verduidelijkingsvragen blijven herkenbaar; zonder gegenereerd antwoord blijft
de antwoordbeoordeling open. De nieuwe maat is afzonderlijk geversioneerd als
`retrieval-task-metrics-v1`; de historische v2-beoordelaar verandert niet.

Activiteituitbreiding kan maximaal top-k verschillende startactiviteiten bereiken.
Het rapport berekent daarom ook het kleinste aantal benodigde bronactiviteiten
over alle AND/OR-bewijskeuzes. Meer benodigde activiteiten dan beschikbare
startpassages bewijst een onhaalbaar bewijsdoel. Onder die grens blijft
haalbaarheid onbekend: de tokenlimiet en filters kunnen nog steeds blokkeren.
Tekstueel uitsluitingsbewijs ontbreekt soms door filtering; dat maakt een nog
niet gegenereerd antwoord niet automatisch fout.

Het rapport bevat per rij de dataset, frozen referentiehash, getypeerde-inputhash,
exacte configuratie, scenario en filtereffect. Referentierollen en verwachte
acties worden pas bij beoordeling/rapportage geladen. Zij worden nooit aan de
zoekfunctie of het antwoordmodel gegeven.
