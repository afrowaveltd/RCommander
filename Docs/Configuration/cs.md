# Konfigurace

*Konfigurace ▸ Možnosti* upravuje vše; změny se projevují živě a do
`settings.toml` se zapíší po potvrzení.

| Sekce | Co ovládá |
| --- | --- |
| Zobrazení | režim zobrazení, skryté soubory, položku `..`, velikost písma, výšku řádku, ikony, náhledy, přirozené řazení |
| Jazyk | jazyk rozhraní, náhled směru |
| Barvy | světlý/tmavý motiv, barvy kurzoru a označení, barvy podle typu souboru |
| Editor | vestavěný editor nebo externí příkaz, písmo, šířka tabulátoru, konce řádků, zalamování |
| Archivy | výchozí formát archivu, úroveň komprese, přípony archivů |
| FTP | pasivní režim, MLSD, kódování, časové limity, uložená připojení |
| Agent | zapnutí MCP serveru, přenos, port, token, politika potvrzování, MCP klienti |
| Zkratky | celá klávesová mapa, upravitelná řádek po řádku |

## Konfigurační soubor

`settings.toml` je určen i k ruční úpravě. Neznámé klíče se zachovají, takže se
soubor z novější verze stále načte. Soubor vznikne při prvním spuštění a přepíše
se po volbě *Konfigurace ▸ Uložit nastavení*.

## Přenosné použití

Nastavte `RCOMMANDER_CONFIG_DIR` na adresář vedle spustitelného souboru a celá
konfigurace — nastavení, oblíbené adresáře, historie, uložená FTP připojení —
zůstane s vaší instalací, například na USB disku.

## Obnovení výchozích hodnot

*Konfigurace ▸ Možnosti ▸ Nebezpečné* obsahuje tovární obnovení. Každá položka se
ještě jednou zeptá, a seznam u ní říká, co přesně se ztratí:

| Obnovení | Co smaže |
| --- | --- |
| Tovární nastavení | všechna nastavení, klávesovou mapu i soubor nastavení |
| Záložky, historie a hotlist | přebytečné záložky, navštívené adresáře, historii příkazů, hotlist |
| Důvěryhodné klíče SSH | `known_hosts`, takže příští připojení se zeptá znovu |
| Uložené přihlašovací údaje | `secrets.enc` včetně hlavního hesla |
| Vše | vše výše uvedené a navíc uloženou relaci |

První dvě jsou neškodné: na soubory na disku nesahají. Zbývající tři mažou data,
která už nelze obnovit — o to právě jde: zapomenuté hlavní heslo nemá jinou cestu
než úložiště smazat.

Ruční smazání `settings.toml` odpovídá první položce; pokud jste si přizpůsobili
klávesovou mapu, nejprve si pořiďte kopii.
