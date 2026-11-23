# Porovnání adresářů

`Shift+F2` porovná oba panely a u každého názvu oznámí, zda jsou položky shodné,
odlišné, nebo existují jen na jedné straně.

## Co se porovnává

U každého názvu nalezeného na kterékoli straně se porovnává druh (soubor nebo
adresář), velikost a čas změny. Výsledkem je tabulka:

| Značka | Význam |
| --- | --- |
| `=` | na obou stranách a shodné |
| `!=` | na obou stranách, ale odlišné |
| `<` | jen ve zdrojovém panelu |
| `>` | jen v cílovém panelu |
| `<DIR>` | adresář, který existuje jen na jedné straně |

Odlišné položky se v aktivním panelu označí, takže `F5` (kopírovat), `F8`
(smazat) nebo `Ctrl+M` (hromadné přejmenování) mohou hned pracovat přesně
s těmito soubory. Stavový řádek hlásí, kolik položek se liší.

## Porovnat, nebo synchronizovat?

*Porovnání adresářů* je rychlá kontrola bez zápisu, která nechává vše ve vašich
rukou: označí rozdíly a nic dalšího nedělá. *Synchronizace adresářů* jde dál a
umožní rozdíly zkopírovat oběma směry, se zkušebním během a výsledky po
souborech.

Porovnání použijte, když chcete vědět, co se změnilo; synchronizaci, když mají
obě strany skončit stejné.

## Rychlé porovnání obsahu

Chcete-li mít jistotu u souborů, které podle velikosti a data vypadají stejně,
ale mohou se lišit, otevřete je po porovnání v prohlížeči (`F3`), nebo použijte
dialog synchronizace s volbou *porovnávat obsah*. Časy nejsou vždy spolehlivé —
soubor zkopírovaný nástrojem, který nezachovává časy, se bude jevit jako odlišný,
i když jsou bajty stejné.
