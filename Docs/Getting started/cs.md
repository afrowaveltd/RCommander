# Začínáme

RCommander je dvoupanelový správce souborů. Vedle sebe jsou dva adresářové
panely, jeden z nich je *aktivní* (jeho kurzor je zvýrazněn). Téměř každá
operace pracuje s aktivním panelem a jako cíl používá ten druhý.

## První spuštění

1. Levý panel se otevře v domovském adresáři, pravý panel také — změnit je
   můžete pomocí `Alt+F1` / `Alt+F2`.
2. Pohybujte se šipkami, `PgUp`/`PgDn`, `Home`/`End`.
3. `Enter` otevře adresář pod kurzorem (a archiv, je-li to povoleno).
4. `Tab` přepíná aktivní panel.
5. Pruh funkčních kláves ve spodní části je nejkratší cesta k hlavním
   operacím: zobrazit, upravit, kopírovat, přesunout, vytvořit, smazat.

## Kde jsou nastavení

Všechna nastavení jsou v jediném člověkem čitelném souboru:

- Linux: `~/.config/rcommander/settings.toml`
- Windows: `%APPDATA%\RCommander\settings.toml`
- Android: privátní konfigurační adresář aplikace

Nastavte proměnnou prostředí `RCOMMANDER_CONFIG_DIR`, chcete-li mít konfiguraci
vedle programu (přenosná instalace).

## Tipy

- Napsáním písmene se kurzor přesune na první soubor začínající tímto písmenem;
  dalším psaním hledání zpřesňujete. `Esc` hledání zruší.
- `Ctrl+D` zobrazí seznam oblíbených adresářů, `Alt+Down` historii adresářů.
- `F1` otevře tuto nápovědu; témata jsou běžné textové soubory, které můžete
  upravovat.
