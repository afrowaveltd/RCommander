# Panely a navigace

## Dva panely

Aktivní panel kreslí kurzor převráceně. Stavový řádek aktivního panelu ukazuje,
kolik obsahuje souborů a adresářů, jejich celkovou velikost a při označení také
počet a velikost označených položek.

## Navigace

| Klávesa | Akce |
| --- | --- |
| `Enter` | otevře adresář / archiv pod kurzorem |
| `Backspace`, `Ctrl+PgUp` | přejde do nadřazeného adresáře |
| `Alt+Left`, `Alt+Right` | předchozí / následující adresář z historie |
| `Alt+F1`, `Alt+F2` | výběr disku či svazku pro levý / pravý panel |
| `Ctrl+R`, `F2` | znovu načte adresář |
| `Ctrl+Left/Right` | zobrazí adresář pod kurzorem v druhém panelu |
| `Tab` | přepne aktivní panel |
| `Ctrl+U` | prohodí oba panely |
| `Ctrl+Shift+U` | nastaví cílový panel na adresář zdrojového panelu |

## Režimy zobrazení

| Klávesa | Režim |
| --- | --- |
| `Ctrl+F1` | stručný — jen názvy, zalamované do sloupců |
| `Ctrl+F2` | podrobný — název, velikost, datum, atributy |
| `Ctrl+Shift+F2` | podrobný včetně komentáře souboru |
| `Ctrl+Shift+F1` | náhledy |
| `Ctrl+Q` | panel rychlého náhledu |

## Řazení

`Ctrl+F3` název, `Ctrl+F4` přípona, `Ctrl+F5` čas, `Ctrl+F6` velikost,
`Ctrl+F7` neřazeno. Druhým stiskem stejné klávesy se pořadí obrátí. Názvy se
řadí přirozeně, takže `file2` je před `file10`.

## Filtrování

Maska v panelu (`*.txt;*.md`) skryje soubory, které jí neodpovídají; adresáře
zůstávají vždy viditelné. `Ctrl+F10` opět zobrazí všechny soubory.

## Rychlý náhled a strom

`Ctrl+Q` nahradí neaktivní panel rychlým náhledem souboru pod kurzorem: obrázky
se zobrazí jako obrázky, text jako text (s rozpoznaným kódováním a podporou
zprava doleva) a neznámé binární soubory jako hexdump. Dalším `Ctrl+Q` se vrátí
seznam souborů.

`Ctrl+Shift+F8` zobrazí místo rychlého náhledu strom adresářů, což výrazně
zrychlí pohyb v hluboké hierarchii složek.

## Výběr disku

Na začátku cesty každého panelu je **výběr svazku**. Ve Windows vypíše všechny disky jako
`C: Systém`, `D: Data`, nebo jen `C:`, když disk nemá jméno. V Linuxu vypíše jen souborové
systémy, které uživatel opravdu použije — odpojitelná média, síťové share, optické disky — a
vynechá systémový kořen, obrazy `snap` a pseudosystémové systémy, protože seznam plný
`/snap/core22/3129` nikomu nepomůže. Kořen zůstává dostupný napsáním `/` do cesty.

Jména pocházejí ze samotného svazku (`/dev/disk/by-label` v Linuxu, informace o svazku ve
Windows), jinak se zobrazí přípojný bod.
