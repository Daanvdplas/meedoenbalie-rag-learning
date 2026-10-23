# Zes vooraf vastgelegde vergelijkingsvragen

`heldout-questions.json` en `heldout-expectations.json` gebruiken hetzelfde schema als de twaalf ontwikkel-/pilotvragen. De vragen gaan over benodigdheden bij drie activiteiten, binnen op dinsdag, niet beschreven Franstalige hulp, een afstandsgrens, twee afspraken en ontbrekend slechtweerbeleid. Het zijn nieuwe samengestelde intents op reeds bekende bronfeiten, geen nieuw domein en geen garantie tegen iedere vorm van overlap.

De zes intentfamilies staan buiten de pilotfamilies. De dataset is klein en doelbewust gekozen; resultaten zijn een verkennende vergelijking tussen vier armen en geen schatting van algemene betrouwbaarheid. Roostertijden garanderen geen vrije plaats. Een niet beschreven taal of aanpassing wordt niet als onmogelijk beschouwd.

T03 combineert een expliciete garantie voor Franse begeleiding bij twee activiteiten; taalhulp en onbekende talen zijn als losse bronfeiten al bekend uit de pilot. T05 combineert twee afspraken; de losse tijd- en adresfeiten zijn bekend. De hold-outclaim geldt dus voor de vooraf apart gehouden samenstellingen, niet voor ongeziene feiten. Bij T02 is de fietssectie S1 relevant als werkplaatscontext, maar zij bewijst op zichzelf geen volledige dinsdag-binnencombinatie: daarvoor blijven S2 en S3 samen nodig.

`heldout-freeze.json` bindt de exacte vragen en criteria met hun compacte schema-JSON-hashes, vóór de eerste live uitvoering. De generator krijgt alleen de vraag en werkelijke context; deze verwachtingen blijven buiten de generator. De onafhankelijke labelreview moet vóór live gebruik zijn afgerond. Menselijke ijking is uitgesteld en wordt niet geclaimd.

Na de eerste uitvoering worden op deze zes vragen geen generatieprompts of retrievalinstellingen afgestemd voor een nieuwe ongeziene claim. Een gevonden fout mag worden onderzocht, maar een aangepaste run heet dan ontwikkeling. Een echte labelfout krijgt een verklaarde nieuwe versie; het oorspronkelijke artifact blijft behouden.
