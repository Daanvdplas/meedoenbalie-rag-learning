# User journey: activiteitenmatch bij de Meedoenbalie

## Use case
Een medewerker wil met een bewoner passende activiteiten bespreken. De demo zoekt in fictieve bewonersnotities en een fictief activiteitenaanbod, en toont mogelijke matches met bewijs en open vragen.

Dit project is een voorstel voor een AI-hulpmiddel. De openbare bronnen beschrijven de bestaande werkwijze; ze zeggen niet dat COA dit hulpmiddel al gebruikt. De invulling kan per locatie verschillen.

## Onderbouwde basisreis

1. **Aanbod verzamelen.** De activiteitenbegeleider en samenwerkingspartners houden zicht op activiteiten binnen en buiten de opvanglocatie.
2. **Wensen bespreken.** De medewerker verkent met de bewoner diens interesses, mogelijkheden en eventuele belemmeringen. Deelname aan activiteiten is vrijwillig.
3. **Een activiteit kiezen.** Bewoner en medewerker bespreken wat past, rekening houdend met praktische zaken zoals tijd, taal en afstand.
4. **Deelname voorbereiden.** Bij een keuze krijgt de bewoner informatie over wat, waar en wanneer. Waar nodig kan iemand ondersteunen bij de eerste keer.
5. **Ervaring opvolgen.** Na de start wordt nagegaan hoe het gaat en of aanpassing of verdere ondersteuning nodig is.

Deze stappen zijn gebaseerd op de [Aan de Slag-handleiding](https://www.pharos.nl/kennisbank/handboek-aan-de-slag/) en de [Aan de Slag-atlas](https://www.pharos.nl/wp-content/uploads/2021/10/Aan-de-Slag-Atlas-20211027.pdf). Zie ook de [COA Jaarplan 2026](https://coa.maglr.com/coa-jaarplan-2026/overzicht-7-succesfactoren), dat inzet op meer deelname aan activiteiten, en [Rijksoverheid over de Meedoenbalie](https://www.rijksoverheid.nl/actueel/weblogs/jenv-verhalen/2026/participatie-vanaf-dag-een-bij-de-meedoenbalie-van-het-coa).

## Voorgestelde AI-laag

1. De medewerker opent een fictief bewonersprofiel en vraagt welke activiteiten mogelijk passen.
2. RAG zoekt relevante passages in fictieve gespreksnotities en activiteitbeschrijvingen.
3. De demo toont een korte lijst met matches, waarom ze mogelijk passen, bronverwijzingen en ontbrekende informatie.
4. De medewerker controleert de bronnen en bespreekt opties met de bewoner. De demo doet geen aanmelding of dossiermutatie.

De architect werkt de volledige minimale agent harness uit: hoe de agent taken doorloopt, context en state beheert, tools aanroept, via RAG informatie ophaalt, bronnen toont, grenzen en fouten afhandelt en uitvoering logt. Evals toetsen het gedrag van die hele keten. Technische keuzes blijven aan de architect.

## Voorbeeld van het scherm

```text
MOGELIJKE MATCH · Buurtmoestuin
Dinsdag 10:00–12:00 · 1,2 km

Waarom getoond?
• Bewoner noemde tuinieren als interesse. [gespreksnotitie]
• Dinsdagochtend past volgens het profiel. [beschikbaarheid]
• Een begeleider kan de eerste keer meegaan. [activiteitbeschrijving]

Nog onbekend: zijn er op dit moment plekken?

[Bespreek met bewoner]  [Bekijk bronnen]
```

## Demo en evals

Laat in een korte demo zien hoe de medewerker een vraag stelt, hoe de agent die afhandelt, een match met onderliggende bronnen controleert en ziet dat actuele beschikbaarheid onbekend is. Evalueer de hele harness: juiste taakafhandeling en toolkeuze, relevante retrieval, onderbouwde claims, respect voor praktische voorwaarden, nette foutafhandeling en geen verzonnen gegevens.

Gebruik alleen synthetische data. Maak evals voor onder meer een goede match, geen passende match, verouderde of ontbrekende beschikbaarheid en informatie die niet in de notities staat. De demo mag geen echte bewonersgegevens of interne COA-documenten bevatten.
