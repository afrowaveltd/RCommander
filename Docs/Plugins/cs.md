# Pluginy

Plugin udělá z programu panel, aniž by se cokoli kompilovalo. Celý plugin je jeden soubor TOML
jménem `plugin.toml` a panel, který se objeví, je stejná dvojice panelů, kterou už znáte: levá a
pravá polovina okna s vlastními sloupci, vlastními řádky a vlastními klávesami.

## Kde plugin bydlí

| Adresář | K čemu |
| --- | --- |
| `<config>/plugins/<id>/plugin.toml` | co jste nainstalovali *vy*. Vyhrává nad přibaleným pluginem se stejným id |
| `plugins/` vedle aplikace | příklady, které se dodávají s RCommanderem |

`<config>` je váš uživatelský konfigurační adresář — na Linuxu `~/.config/rcommander`, na Windows
`%APPDATA%\rcommander`. Složka, jejíž `plugin.toml` nejde přečíst, se ohlásí ve stavovém řádku a
přeskočí; nikdy nezabrání startu aplikace.

**Jméno složky se musí rovnat id**, protože id je schéma URL: na `notes://` odpovídá
`plugins/notes/`. Znovu načíst adresář je příkaz `cm_ReloadPlugins`, takže nový plugin nepotřebuje
restart.

## Nejmenší fungující plugin

```toml
[plugin]
id = "notes"                    # malá písmena, musí se rovnat jménu složky
name = "Notes"
version = "1.0"
description = "Každá poznámka v Markdownu jako řádek panelu"

[source]
name = "Notes"                  # popisek v řádku cesty
list = ["./list.sh"]            # vypíše JSON pole objektů
open = ["./open.sh", "{path}"]  # nepovinné; {path} je klíč řádku
key = "path"                    # které pole JSONu řádek identifikuje

[[source.columns]]
id = "title"                    # první sloupec je název řádku
label = "Poznámka"
field = "title"
width = 300

[[source.columns]]
id = "words"
label = "Slova"
field = "words"
numeric = true

[source.capabilities]
allow = ["view"]                # seznam, který si můžete přečíst, a nic víc
```

Celá spustitelná verze je v `plugins/examples/notes/` — zkopírujte tu složku do
`<config>/plugins/notes` a otevřete `notes://`.

## Co musí vypisovací příkaz vypsat

JSON pole objektů, jeden objekt na řádek:

```json
[
  {"path": "/home/já/Poznámky/ahoj.md", "title": "ahoj", "words": 10},
  {"path": "/home/já/Poznámky/todo.md", "title": "todo", "words": 3}
]
```

Cokoli jiného je chyba, která řekne, co přišlo — prázdný panel nikdy nenahrazuje rozbitý příkaz,
protože „nástroj neodpověděl" a „tady nic není" nesmí vypadat stejně.

## Argumenty a proč tam není shell

`list` a `open` jsou pevná pole. V celém manifestu je právě jedna substituce: `{path}` v `open`
dostane klíč řádku. Hodnota se předává jako **jeden argument**, takže poznámka jménem `; rm -rf ~`
je název a ne instrukce. Kdo chce shell, musí ho jmenovat sám (`["sh", "-c", "…"]`) — a pak je to
jeho shell a jeho pravidla.

Příkazy se spouštějí ve složce pluginu, což je to, co dovolí nést si vlastní pomocný skript
(`./list.sh`) místo spoléhání na něco nainstalovaného v systému.

## Sloupce

- `id` — stabilní uvnitř pluginu a nikdy takové, které panel už používá (`name`, `ext`, `size`,
  `date`, `attrs`, `comment`, `mode`, `link`). Použití takového id se odmítne podle jména.
- `label` — text hlavičky.
- `field` — pole JSONu, které buňku naplní. Výchozí je `id`.
- `width` — preferovaná šířka v bodech. První sloupec vždy vyplní zbytek panelu.
- `numeric` — zarovnat vpravo, pro čísla.

Hlavičky sloupců pluginového zdroje nejsou tlačítka pro řazení: panel neumí porovnat dvě poznámky
tak, jako porovná dva názvy souborů.

## Schopnosti

`allow` vypisuje souborové operace, které v tomto místě existují: `copy`, `move`, `create`,
`delete`, `rename`, `view`. **Výchozí stav je nic** a právě v tom je celý smysl: plugin, který
nedeklaruje `delete`, nemůže mít smazané řádky. Klávesa se vykreslí jako nedostupná, příkaz se
odmítne dřív, než se cokoli zařadí, a neobjeví se žádný schvalovací dialog — protože není co
schvalovat.

Ani deklarovaná schopnost není oprávnění. Pluginový zdroj je *místo*, a i tam, kde mazání
deklaruje, rozhoduje politika, schvalovací dialog a audit stejně jako u místního souboru.

## Co zatím nejde

- **Akce řádku** (položka *Odpovědět* nebo *Exportovat* v místní nabídce) umí zdroje kompilované
  do aplikace, ale manifest je zatím neumí deklarovat.
- **MCP zdroje**: stejný manifest, jen `list` jmenuje nástroj místo programu, je další krok. Nic
  z toho, co je výš, se přitom nezmění.
- **Zápis**: manifestový zdroj neumí vytvářet, přesouvat ani mazat; `allow` jen říká, co by mělo
  smysl, kdyby mohl.
