# Editor (F4)

`F4` otevře soubor pod kurzorem ve vestavěném editoru, `Shift+F4` nejprve vytvoří
nový soubor. Místo něj lze nastavit externí editor — funguje jakýkoli příkaz se
zástupným symbolem `%1` pro název souboru.

## Ovládání okna

Editor (stejně jako chat s agentem) má vlastní lištu v pravém horním rohu: **min**,
**max**, **pin** a **x**.

- **min** okno složí na titulkovou lištu, takže přestane zakrývat panely, ale
  napsaný text zůstane;
- **max** vyplní plochu obou panelů a při změně velikosti hlavního okna se přizpůsobí;
- **pin** udělá totéž a drží okno vpředu po celou dobu práce, se zvýrazněným
  rámečkem, aby bylo vidět, které okno je připnuté;
- **x** okno zavře. Text zůstává v paměti do zavření; neuložené změny se před
  zavřením ohlásí.

## Co editor umí

- načítá a ukládá text v UTF-8, rozpozná původní kódování a zachová konce řádků,
  se kterými byl soubor otevřen,
- `Enter` vkládá konce řádků dokumentu, ne platformy, takže unixový soubor
  zůstane unixovým,
- ptá se před zahozením neuložených změn,
- ve stavovém řádku ukazuje pozici kurzoru a velikost souboru.

## Když do souboru nelze zapisovat

Okno varuje před psaním, ne až po neúspěchu: *do tohoto souboru nelze zapisovat;
uložení nabídne kopii nebo zvýšení práv*. Uložení pak ohlásí chybu systému a nabídne
dvě poctivé cesty:

- **Uložit kopii…** zapíše text do složky Dokumenty se stejným názvem (a `(1)`,
  `(2)`…, pokud tam už je — kopie nikdy nic nepřepíše). Okno pak ukáže cestu kopie,
  protože originál zůstal beze změny.
- **Uložit jako správce** vyžádá od systému zvýšená práva: `pkexec` na Linuxu, UAC
  na Windows, dialog správce na macOS. Když zvýšení práv není k dispozici, hlášení to
  řekne, místo aby předstíralo pokus.

Nic se nikdy neohlásí jako uložené, když uložené není.

## Klávesy

| Klávesa | Akce |
| --- | --- |
| `Ctrl+S` | uložit |
| `Ctrl+Z` / `Ctrl+Y` | zpět / vpřed |
| `Ctrl+F` | hledat |
| `Ctrl+G` | přejít na řádek |
| `Ctrl+A` | vybrat vše |
| `Tab` / `Shift+Tab` | odsadit / zrušit odsazení výběru |
| `Esc` | zavřít (s dotazem na neuložené změny) |

## Úpravy souborů v archivech

Soubor otevřený z archivu se rozbalí do dočasného umístění; při uložení se archiv
přebalí s novou verzí. Totéž platí pro soubory otevřené z FTP panelu: uložení
nahraje novou verzi zpět.


## Umístění v pracovním prostoru

Výchozí zobrazení překryje oba panely. Ovládací prvky umožňují přesun do
levého či pravého panelu nebo do plovoucího okna uvnitř hlavní aplikace.
Přesun nemění otevřený soubor ani rozepsaný text. Druhý panel zůstává
použitelný; kliknutím jej aktivujete. Samostatné okno operačního systému
ani integrovaný terminál zatím nejsou součástí této změny.
