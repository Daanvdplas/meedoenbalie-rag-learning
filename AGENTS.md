# COA Meedoenbalie-project

Lees eerst [user_journey.md](user_journey.md). Dit document beschrijft de use case, de onderbouwde gebruikersreis en de voorgestelde AI-laag.
Gebruik voor architectuur- en platformkeuzes ook [agent-harness-landscape.md](agent-harness-landscape.md) als gedateerd startpunt.

## Doel
Bereid met de architect een kleine, complete agent harness voor die een Meedoenbalie-medewerker helpt passende activiteiten te vinden op basis van bewonersnotities en het activiteitenaanbod. De demo moet laten zien hoe de agent wordt aangestuurd, welke tools en gegevens hij gebruikt, hoe de uitvoering wordt gevolgd en hoe gedrag wordt geëvalueerd.

## Afspraken
- Gebruik uitsluitend verzonnen bewoners- en activiteitendata.
- Houd de beschreven COA-werkwijze en onze voorgestelde AI-functie duidelijk uit elkaar; uitvoering kan per locatie verschillen.
- Toon bij elke suggestie de gebruikte bron. Laat ontbrekende of verouderde informatie als onbekend zien.
- De medewerker bespreekt suggesties met de bewoner; meedoen blijft vrijwillig.
- De demo schrijft niets naar dossiers, verstuurt geen berichten en meldt niemand aan.
- Werk met de architect de hele minimale harness uit: agentloop en taakafhandeling, context en state, tools, RAG, bronvermelding, grenzen en foutafhandeling, logging/tracing en evals.
- Houd de harness klein en uitlegbaar. Kies de technische aanpak met de architect; begin daarna pas met bouwen.
