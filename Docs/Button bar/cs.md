# Tlačítková lišta

Tlačítková lišta dává nejpoužívanějším příkazům viditelné místo ovladatelné
myší. Jsou dvě: vodorovná a svislá, zobrazit lze kteroukoli z nich.

## Přidání tlačítka

*Konfigurace ▸ Tlačítková lišta* otevře editor. Každé tlačítko má:

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
