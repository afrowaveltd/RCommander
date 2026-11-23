# Editor (F4)

`F4` otevře soubor pod kurzorem ve vestavěném editoru, `Shift+F4` nejprve vytvoří
nový soubor. Místo něj lze nastavit externí editor — funguje jakýkoli příkaz se
zástupným symbolem `%1` pro název souboru.

## Co editor umí

- načítá a ukládá text v UTF-8, rozpozná původní kódování a zachová konce řádků,
  se kterými byl soubor otevřen,
- `Enter` vkládá konce řádků dokumentu, ne platformy, takže unixový soubor
  zůstane unixovým,
- ptá se před zahozením neuložených změn,
- ve stavovém řádku ukazuje pozici kurzoru a velikost souboru.

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
