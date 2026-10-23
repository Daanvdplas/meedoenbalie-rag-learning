# Meetlat voor de activiteitenproef

De twaalf gevallen in `fixtures/experiments/questions.json` zijn een instrumentpilot op vijf fictieve documenten, geen bewijs voor algemene RAG-kwaliteit. P01–P06 zijn al bekende ontwikkelgevallen. P07–P12 verbreden de pilot over fietsen, taal, koken, wandelen en ontbrekend aanbod. Er zijn nog geen ongeziene uitvoeringen of menselijke referentieoordelen vastgelegd.

## Input en verwachtingen

`questions.json` bevat vragen, versie, referentiedatum en exacte bronhashes. Alleen de vraagtekst mag naar de generator; familie, split en type zijn evaluatiemetadata. `expectations.json` bevat afzonderlijk de betekeniscriteria en een testcanary. Het wordt nooit geïndexeerd of aan de antwoordgenerator aangeboden. De afzonderlijke beoordelaar mag criteria pas na generatie ontvangen. Een gecontroleerde test van de echte requestgrens volgt in de runner; deze datasetvalidator alleen bewijst die grens niet.

De corpusfingerprint gebruikt dezelfde exacte UTF-8-documenten en scheiding als de bestaande indexmanifest. `questions_sha256` is de SHA-256 van compacte UTF-8-JSON in de veldvolgorde van het versie-1-`QuestionSet`-schema. Daardoor veranderen vragen en metadata de binding met de verwachtingen, maar witruimte in het JSON-bestand niet. Veranderde bronbytes of referentiedatum vereisen een nieuwe datasetversie en review; oude resultaten blijven bij hun oude manifest.

## Betekenis van bewijs

Een feit heeft een beschrijving, geen verplichte antwoordzin. Binnen één `evidence_alternatives`-groep zijn alle secties nodig (AND); iedere volledige alternatieve groep mag het feit dragen (OR). Secties hebben altijd document-ID én sectie-ID: het S2 van tuinieren is niet het S2 van fietsen.

Een expliciete onbekende kan bewijs hebben: de wandelbron zegt bijvoorbeeld letterlijk dat rolstoeltoegankelijkheid niet beschreven is. Bij ophalen voor tuinieren of badminton op zaterdag bestaat geen positieve bewijssectie. Dat staat uitdrukkelijk als `kind: unknown` met lege alternatieven en een verplicht onbekend antwoord. Zo'n ontbrekende noemer is geen retrievalscore nul. Bestaat verwacht bewijs wel maar wordt het niet opgehaald, dan is de dekking nul.

`relevant_sections` bevat secties die de gevraagde feiten of concrete voorwaarden direct ondersteunen, inclusief bruikbaar uitsluitingsbewijs. Bij de volledige dinsdaglijst zijn alle vijf dagsecties relevant voor het controleren welke activiteiten wel en niet passen. Voor P06 is de relevantielijst bewust niet volledig: precision is daar N.v.t. Voor de overige gevallen is de lijst over alle dertig secties nagelopen; de onafhankelijke reviewer controleert die labels. De waardering blijft een expliciete taakgebonden annotatie, geen objectieve eigenschap van de passage.

## Menselijke ijking

Open `human-review.html` in deze map. Het blad bevat acht handgemaakte kandidaatantwoorden met exacte bronfragmenten, geen live modelresultaten en geen voorgesteld oordeel. H01–H04 zijn voor het ontwikkelen van de rubric; H05–H08 controleren de beoordelaar afzonderlijk. Goed kan ook een terecht onbekend antwoord of noodzakelijke verduidelijking zijn. Beoordeling nodig betekent dat de menselijke beoordelaar niet voldoende kan beslissen.

De knop kopieert de zelf gekozen labels naar JSON om in de chat terug te sturen. Er is geen opslag of netwerkverzoek. De lege `human_label`-velden in `human-review.json` blijven leeg totdat een echte mens de labels aanlevert. AI-consensus is geen menselijke ijking. De kleine ijkset bewijst nog geen betrouwbare beoordelaar voor andere onderwerpen.

De gebruiker heeft menselijke ijking voor nu uitgesteld. `ai-reference.json` bevat daarom afzonderlijke, brongebonden voorlopige referenties van een andere AI-agent, gecontroleerd op case-ID's, broninhoud en worksheetfingerprint. `origin: ai_agent` en `human_calibration: false` blijven expliciet. Overeenstemming hiermee meet alleen overeenstemming met deze AI-referentie; inhoudelijke kwaliteitsoordelen blijven voorlopig. Het oorspronkelijke menselijke blad en de lege menselijke labels veranderen hierdoor niet.

## Nieuwe evaluatievragen

Een ongeziene vergelijking krijgt eigen vraag- en verwachtingsbestanden, datasetversie en vooraf gereviewde criteria. Houd een vraagfamilie met alle parafrases in dezelfde split; bekende missers worden niet achteraf ongezien genoemd. Leg selectie, bevriesmoment en eventuele wijzigingen vast vóór uitvoering. Verander na een slechte score geen verwachtingen om de score te verhogen. Een geconstateerde labelfout krijgt een verklaarde nieuwe versie; beide resultaten blijven zichtbaar.

Deze indeling past de principes van gescheiden vraagdekking en ijking toe; zij implementeert geen formele garantie van de volledige onderzoeksmethoden. Bronnen: [Coverage, Not Averages, ICML 6–11 juli 2026](https://proceedings.mlr.press/v306/klearman26a.html) en [How to Correctly Report LLM-as-a-Judge Evaluations, ICML 6–11 juli 2026](https://proceedings.mlr.press/v306/lee26bl.html).
