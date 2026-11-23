# Synchronizace adresářů

*Příkazy ▸ Synchronizovat adresáře* porovná oba panely a ukáže soubor po
souboru, co se liší. Hodí se na zrcadlení složky, zálohu nebo porovnání místního
adresáře s FTP serverem.

## Porovnání

U každého názvu, který se objeví na některé straně, vidíte velikosti a časy obou
kopií a značku směru:

| Značka | Význam |
| --- | --- |
| `=` | shodné (velikost i čas) |
| `->` | jen v levém panelu — kopírovat doprava |
| `<-` | jen v pravém panelu — kopírovat doleva |
| `!=` | na obou stranách, ale odlišné |

Volby určují, co se počítá jako odlišnost: ignorovat data, porovnávat obsah
bajt po bajtu nebo běžet asymetricky (kopírovat jen zleva doprava). *Zkušební
běh* ukáže plán, aniž by sáhl na jediný soubor.

## Spuštění

Označte řádky, které chcete, a stiskněte *Synchronizovat*. Každá kopie se
objeví v seznamu průběhu s vlastním výsledkem a chyby se hlásí po souborech,
místo aby zrušily celou dávku.

## Pozor na časová pásma u FTP

Je-li jedna strana FTP server, může mít server jiné časové pásmo. Pokud server
neposkytuje spolehlivý čas změny, mohou se data u shodných souborů jevit jako
rozdílná — v takovém případě porovnávejte podle obsahu.

## V dialogu

*Porovnat* (stejný algoritmus jako `Shift+F2`, ale na dvou cestách zadaných
v dialogu) naplní tabulku; *Synchronizovat* poté zkopíruje každý řádek se značkou
`->`, `<-` nebo `!=` ve směru, kterým značka ukazuje. U `!=` vyhrává novější kopie.
*Zkušební běh* ukáže plán a na žádný soubor nesáhne. *Ignorovat data* považuje
položky se stejnou velikostí za shodné, což se hodí, když jsou časy nespolehlivé —
například hned po kopírování bez zachování časů.
