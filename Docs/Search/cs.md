# Hledání

`Alt+F7` otevře dialog hledání. Hledá se v adresáři aktivního panelu a je-li
zapnuto *včetně podadresářů*, rekurzivně ve všem pod ním.

## Co lze hledat

- **maska názvu** — zástupné znaky jako `*.log;*.tmp`, nebo prostý podřetězec,
  pokud zástupné znaky neobsahuje,
- **obsah souboru** — textový vzor, volitelně regulární výraz, volitelně
  rozlišování velikosti písmen nebo jen celá slova,
- **uvnitř archivů** — archivy se prohledají bez rozbalení.

## Výsledky

Výsledky se zobrazí v seznamu s cestou, velikostí a u nalezeného obsahu
i s odpovídajícím řádkem. `Enter` na výsledku otevře jeho adresář v druhém panelu
a postaví kurzor na soubor, takže můžete hned jednat.

## Omezení

Hledání se zastaví po nastavitelném počtu výsledků (výchozí 10 000) a lze je
kdykoli přerušit. Skryté soubory a symbolické odkazy se procházejí jen na
výslovné zapnutí, což brání nekonečným smyčkám.

## Spuštění hledání

Stiskněte *Start*. Hledání běží ve vlákně na pozadí: seznam výsledků roste, okno
zůstává ovladatelné a tlačítko se změní na *Stop*, takže hledání lze kdykoli
přerušit. Dvojklikem (nebo kliknutím) na výsledek se otevře jeho adresář v druhém
panelu s kurzorem na souboru, připravený pro `F3`, `F5` nebo `F8`.

Režim regulárního výrazu používá syntaxi knihovny `regex`; neplatný výraz prostě
nic nenajde, místo aby hledání selhalo.
