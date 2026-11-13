# Evaluaties

Alle evaluatiecode, vraagsets, handleidingen en bewaarde resultaten staan hier. De indeling volgt wat een proef meet:

- [RAG](rag/README.md): vinden we de benodigde bronnen, met welke zoekstrategie en hoeveel zoekwerk?
- [COA-assistent](coa-assistent/README.md): klopt het uiteindelijke antwoord, gebruikt het de bronnen goed en blijven onbekende feiten onbekend?
- [Gedeelde vraagsets](shared/README.md): dezelfde vastgelegde vragen en verwachtingen worden door beide soorten evaluaties gebruikt.

De modeluitvoeringen en afzonderlijke AI-beoordelingen blijven via **Codex CLI** lopen. Een lokale retrievaltest of het opnieuw beoordelen van opgeslagen zoektraces heeft geen antwoordmodel nodig. De webdemo gebruikt apart de Responses API.

Productcode en de fictieve activiteitdocumenten blijven in `src/` en `fixtures/`. Gewone softwaretests blijven in `tests/`; evaluatiespecifieke Python-/rapportchecks staan bij hun evaluatie. De bestaande CLI-opdrachten `eval`, `experiment` en `lab` zijn behouden.

## Bewaarde geschiedenis

Ruwe JSON/JSONL, bevroren vraagsets en manifesten zijn byte voor byte verplaatst. Historische padvelden en het verzegelde oktoberarchief behouden daarom hun oude namen; die zijn herkomstinformatie, geen opdracht om de oude mappen opnieuw aan te maken. Actuele instructies staan bij de twee categorieën. De loaders controleren dezelfde oorspronkelijke hashes.

Nieuwe lokale uitvoer kan onder `rag/results/local/` of `coa-assistent/results/local/` worden opgeslagen; die mappen worden door Git genegeerd. Maak voor iedere uitvoering een nieuw bestand en gebruik bij hervatten hetzelfde budgetledger. Alleen een expliciete `--live`-opdracht mag een model starten.
