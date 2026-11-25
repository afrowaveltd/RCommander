# Tlačítková lišta

Tlačítková lišta dává nejpoužívanějším příkazům viditelné místo ovladatelné
myší. Jsou dvě: vodorovná a svislá, zobrazit lze kteroukoli z nich.

## Přidání tlačítka

*Konfigurace → Tlačítková lišta* otevře editor. Každé tlačítko má:

- **příkaz** — interní příkaz jako `cm_Copy`, externí program nebo adresář
  k otevření,
- **popisek** a volitelnou ikonu,
- volitelné **parametry** se zástupnými symboly: `%P` cesta zdrojového panelu,
  `%N` název souboru pod kurzorem, `%T` cílová cesta, `%M` název v cíli,
  `%S`/`%L` seznam označených souborů, `%D` aktuální adresář.

Protože tlačítka, menu, zkratky i nástroje MCP používají stejné identifikátory
příkazů, je tlačítko pohodlný způsob, jak dát automatizaci tvář.

## Ovládání klávesnicí

`Alt+Shift+F11` zaměří vodorovnou lištu, `Alt+Shift+F12` svislou; šipky pak
přecházejí mezi tlačítky a `Enter` spustí to zaměřené.

## Úprava lišty

*Konfigurace → Tlačítková lišta* (nebo tlačítko `…` na pravém konci lišty)
otevře editor. Kliknutím na tlačítko se jeho údaje načtou do polí, tlačítkem `+`
vytvoříte nové:

- **popisek** — co tlačítko zobrazuje; prázdný znamená název příkazu,
- **příkaz** — interní příkaz (`cm_Copy`, `cm_FileSync`, …), adresář nebo externí
  program,
- **parametry** — volitelné, se zástupnými symboly `%P` (zdrojová cesta),
  `%T` (cílová cesta), `%N` (soubor pod kurzorem).

`apply` změnu uloží, `-` označené tlačítko smaže a `up`/`down` ho přesune. Lišta
se kreslí z `[button_bar] buttons` v `settings.toml`, takže stejný soubor lze
upravovat i ručně:

```toml
[[button_bar.buttons]]
caption = "Sync"
command = "cm_FileSync"
parameters = ""
separator = false
```

Každé výchozí tlačítko odkazuje na skutečný interní příkaz; tlačítko s prázdným
příkazem je odmítnuto, aby nevznikalo mrtvé tlačítko.
