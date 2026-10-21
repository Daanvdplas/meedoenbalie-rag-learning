# Gegevens bewaren en wissen in de lokale proef

Gecontroleerd op **21 oktober 2026**. Dit beschrijft de huidige code en grenzen; het is geen toestemming voor echte interne gegevens of een garantie dat alle kopieën verwijderd zijn. Gebruik uitsluitend verzonnen vragen en activiteiten.

## Wat wordt bewaard?

| Gegevens | Plek en doel | Huidige grens | Wie kan wissen, en hoe? |
| --- | --- | --- | --- |
| Vijf fictieve documenten en interne regressievragen | `fixtures/`, Git en via `include_str!` in binaries; bronnen en regressies | Blijven tot de beheerder ze verwijdert; geen automatische termijn | Repositorybeheerder: bron verwijderen, opnieuw bouwen en oude artifacts apart opruimen. Een commit wist geen Gitgeschiedenis of bestaande binary. |
| Modelgewichten, tokenizer en downloadmetadata | `.model-cache/` onder de werkmap; lokale embeddingberekening | Blijven tot handmatig verwijderen | Lokale beheerder: processen stoppen en deze cache verwijderen. Dit zijn modelbestanden, geen opgeslagen document- of vraagvectoren. |
| Bronsecties, documentvectoren en tijdelijke vraagvectoren | RAM van server en eigen MCP-childprocessen; zoeken | Geen database of vectorbestand; MCP-index leeft binnen die MCP-sessie, serverindex tot serverstop | Lokale beheerder: server en bijbehorende childprocessen stoppen. Dit belooft geen wissen uit swap, crashdumps of backups. |
| Vragen, gevonden passages, antwoord, context en eventuele interne beoordelingen | `RunStore` in server-RAM; historie, opnieuw uitvoeren en optioneel rapport | Maximaal 16 sessies en 40 runs per sessie; geen tijdgebonden vervaldatum of automatische opschoning | Lokale beheerder: server herstarten. **Nieuwe vraag** en **Nieuwe sessie** wissen eerdere serverrecords niet. Voor de huidige synthetische proef zijn dit aantallimieten, geen organisatiebeleid voor bewaartermijnen. |
| Sessie-ID en weergegeven inhoud | Browser: `sessionStorage`-sleutel `coa-test-session`; vraag/antwoord in de pagina | Geen expliciete app-TTL; browsergedrag zoals tabherstel kan verschillen. De ID is geen gebruikersidentiteit of toegangsbeleid | Tester: tab sluiten en waar nodig site-/sessiegegevens wissen. Herstart van de server maakt oude IDs onbruikbaar. Browserwissen wist de serverrecords niet. |
| Optioneel TXT-testverslag via `/api/report` | Download bij de tester; vraag, antwoord, status, veilige fout en versie-/model-/hashmetadata | Geen door de app beheerde vervaldatum; export is geen anonieme samenvatting | Ontvanger: bestand en eigen kopieën verwijderen. De app kan gedeelde downloads niet terughalen. De gewone demo toont geen exportknop, de API bestaat voor intern testen. |
| Tijdelijke uitvoerschema's en procesuitvoer | Tijdelijke runmap en begrensd proces-RAM; Codex uitvoeren en JSON-events verwerken | Schema wordt via cleanup opgeruimd bij normale afloop/fout/cancel; harde proces-/machinestop kan resten nalaten. stdout maximaal 512 KiB, stderr maximaal 16 KiB | Lokale beheerder: app stoppen, eigen achtergebleven `coa-run-*`-mappen in de systeem-tempmap gericht controleren en verwijderen. Procescleanup is geen veilige overschrijfgarantie. Ruwe Codex-stderr wordt niet in runrecords of rapporten overgenomen. |
| Terminal-, Codex-/host-, OS- en eventuele crashdiagnostiek | Buiten `RunStore`; uitvoering en ondersteuning | De app schrijft zelf geen vraag-/passagelogbestand. Volledige inventaris en retentie van afhankelijkheden, Codex/OS-diagnostiek, swap en crashrapporten zijn **niet bewezen** | Lokale beheerder en verantwoordelijke dienstbeheerder moeten locaties, toegang, opslagbescherming en wissen afzonderlijk controleren. `--ephemeral` bewijst geen volledige lokale of externe verwijdering. |
| Vraag en door Codex gevonden tekst bij de modeldienst | Externe verwerking via de huidige Codex-aanmelding; antwoord maken | Toepasselijke accountvoorwaarden, traininginstellingen, retentie en wismogelijkheden zijn hier **onbekend** | Account-/organisatie-eigenaar moet dit vóór echte gegevens vaststellen. Serverherstart, lokale cache verwijderen of `--ephemeral` wissen providergegevens niet aantoonbaar. |
| Gitgeschiedenis, oude builds, downloads en backups | Repository, artifactopslag, andere computers en backups | Buiten de RAM-wisgrens; geen automatische verwijdering vanuit deze app | Elke eigenaar afzonderlijk; onbedoelde echte gegevens vereisen stoppen, kopieën inventariseren en een afgesproken incident-/wisaanpak. `.gitignore` is geen toegangscontrole. |

De lokale beheerder is in deze leerproef de eigenaar van de machine. Een verantwoordelijke organisatie moet voor later gebruik zelf eigenaren en toegestane termijnen benoemen; deze tabel verzint geen wettelijke bewaartermijn.

## Welke technische details mogen gedeeld worden?

`CodexRunner::configuration()` publiceert uitsluitend prompt-/schema-/toolbeleidversies, clientversie, modelnaam, reasoning-instelling en SHA-256-fingerprints van Codex, code-mode host en MCP-binary. Absolute runtimepaden blijven intern. Succesvolle CLI-/API-runrecords hebben geen `runtime_path`-veld. Modelnamen zijn bewust gedeelde configuratie; zet daar geen geheimen in.

De runner geeft vaste, verschillende oorzaakcategorieën voor onder meer ontbrekende runtime, tijdelijk bestand, zoekfout, ongeldige uitvoer, uitvoerlimiet en timeout. Ruwe providerdiagnostiek en OS-foutteksten worden niet in die runfouten gekopieerd. Codex-versie-uitvoer moet het verwachte beperkte `codex-cli <versie>`-formaat volgen; extra diagnostiek wordt geweigerd. MCP publiceert een vaste technische zoekfout; invoerfouten en de vier-zoekactieslimiet blijven herkenbaar. Dit is een grens voor technische metadata, **geen algemeen filter voor persoonsgegevens**: de bewust bewaarde vraag, passages en antwoord blijven leesbaar.

Bron-/modelconfiguratie en binaryhashes blijven beschikbaar voor herhalen. Gewijzigde binaries of context worden nog steeds geweigerd. Deze hashes zijn controlemetadata, geen anonimisering of versleuteling.

## Kleine wisprocedure voor de synthetische proef

1. Stop de lokale server en de eigen uitvoeringsprocessen. Bij normale afloop, timeout en cancel ruimt de runner zijn procesgroep op. Herstart geeft een lege `RunStore`; een nieuwe browsersessie alleen doet dat niet.
2. Sluit de testtab en wis zo nodig de site-/sessiegegevens. Verwijder eigen downloads apart. Controleer alleen de bij deze app horende tijdelijke runmappen; wis geen andere systeem- of Codexbestanden op goed geluk.
3. Als broninhoud moet verdwijnen: verwijder de bron, bouw opnieuw en neem oude binaries, Gitgeschiedenis en backups mee in de afgesproken aanpak. Alleen de modelcache verwijderen wist deze kopieën niet.
4. Noteer welke externe kopieën niet aantoonbaar weg zijn. Vraag/gevonden tekst gingen naar de modeldienst; benader daarvoor de verantwoordelijke account-/dienstbeheerder. Claim geen volledige verwijdering.

## Controleerbaar bewijs

`cargo test --locked` gebruikt alleen fictieve canaries en lokale nep-runtimeprocessen voor deze outputgrens: private paden in configuratie/resultaat, stderr bij succes/fout/timeout, ongeldige versie-uitvoer, veilige schemafout en MCP-zoekfout. HTTP-tests controleren historie, run-API en TXT-export voor succes, fout en timeout; fingerprints en geweigerde gewijzigde replaycontext blijven behouden. Deze tests bewijzen geen organisatorische autorisatie, PII-filter, providerretentie of volledige verwijdering. Die open punten blijven onder privacy-hoofdissue [#19](https://github.com/Daanvdplas/meedoenbalie-rag-learning/issues/19), met [#23](https://github.com/Daanvdplas/meedoenbalie-rag-learning/issues/23) als bewaartakenlijst.
