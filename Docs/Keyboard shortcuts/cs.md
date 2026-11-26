# Klávesové zkratky

Všechny zkratky lze změnit v *Konfigurace → Možnosti → Zkratky*; ukládají se do
tabulky `[shortcuts]` v souboru `settings.toml` jako `"ctrl+f3" = "cm_SrcByName"`.

## Základní zkratky

| Klávesa | Akce |
| --- | --- |
| `F1` | nápověda |
| `F2` | znovu načíst adresář |
| `F3` | náhled souboru |
| `F4` | upravit soubor |
| `F5` | kopírovat |
| `F6` | přesunout / přejmenovat |
| `F7` | vytvořit adresář |
| `F8` | smazat |
| `F9` | zabalit |
| `F10` | **ukončit program** — klasické zakončení z Norton Commanderu |
| `F11` | otevřít |
| `F12` | otevřít terminál zde |
| `Tab` | přepnout panel |
| `Enter` | otevřít |
| `Backspace` | nadřazený adresář |
| `Insert` / `Space` | označit |
| `Esc` | zrušit |

## Řada se řídí tím, co má fokus

Funkční klávesy nejsou jedna tabulka, ale čtyři — a lišta dole kreslí řadu té plochy,
která je právě vepředu, se jménem plochy na levém okraji.

| Kde | Co dělají `F1`…`F4` |
| --- | --- |
| **soubory** | nápověda, znovunačtení, náhled, editace — klasická sada až po `F12` = otevřít terminál zde |
| **chat** | nápověda k chatu, volba modelu, otevřít prompty, uložit otázku jako prompt |
| **editor** | nápověda, uložit, uložit kopii, načíst z disku; `F10` zavře |
| **prohlížeč** | nápověda, znovunačtení a tři režimy (`F3` text, `F4` hex, `F5` obrázek), zoom na `F6`/`F7`, `F10` zavře |

Nad konverzací není žádný řádek kurzoru ke kopírování ani mazání, takže ty příkazy v
její řadě **nejsou** — místo aby byly zašedlé: tlačítko, které nemůže fungovat, je
horší než žádné. `F1` je vždy nápověda a otevře téma té plochy, na které jste ji
stiskli — chat, editor, prohlížeč nebo samotné zkratky.

Pod holými klávesami kreslí lišta druhou řadu **variant s modifikátory** — `Shift`,
`Ctrl` a `Alt`. Nejsou to dekorace: najetí myší řekne, který příkaz spustí, a kliknutí
ho spustí. Berou se z klávesové mapy, takže varianta, kterou si přemapujete, se objeví
se svým novým příkazem a ta, která neexistuje, se nenabízí.

`F10` ukončí program, jak to dělal Norton Commander; totéž `Alt+F4`. (Lišta nabídek
je stále vidět, takže žádná klávesa k otevření nabídky není potřeba.)

Řady se čtou z modelu, a proto se lišta a klávesy nemohou rozejít — a budoucí textové
rozhraní bude mít stejnou klávesnici.

## Kombinace

`Ctrl+F1`…`Ctrl+F7` mění režim zobrazení a řazení, `Ctrl+Shift+F5` vytvoří
zástupce souboru, `Ctrl+M` otevře hromadné přejmenování, `Ctrl+D` oblíbené
adresáře, `Alt+Down` historii adresářů, `Alt+F7` hledání, `Alt+F5`/`Alt+F6`
zabalení a rozbalení, `Ctrl+T` novou kartu, `Ctrl+W` kartu zavře a `Ctrl+Tab`
mezi nimi přepíná. `Ctrl+Q` otevře rychlý náhled souboru pod kurzorem,
`Ctrl+Shift+F8` strom adresářů a `Shift+F2` porovná oba panely.

## Funkční klávesy ve spodním pruhu

Pruh ukazuje aktuální význam kláves `F3`…`F8`: mění se podle kontextu, například
`Zobrazit` se u textových souborů změní na `Upravit`. Kliknutí na popisek příkaz
spustí, kliknutí pravým tlačítkem otevře nastavení.
