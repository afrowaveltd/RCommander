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

Smažte `settings.toml` a vrátíte se k výchozímu nastavení; pokud jste si
přizpůsobili klávesovou mapu, nejprve si pořiďte kopii.
