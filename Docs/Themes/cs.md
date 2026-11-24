# Témata

Barvy jsou data, ne kód. Každá barva, kterou aplikace vykreslí, pochází ze souboru
tématu v adresáři `themes/` vedle spustitelného souboru (vývojářsky v kořeni
repozitáře). Upravuje je *Konfigurace ▸ Možnosti ▸ Barvy*; formát souboru je
určený i pro ruční úpravy.

## Formát souboru

```toml
[meta]
name = "Ocean"
base = "dark"          # light, dark, high-contrast nebo jméno jiného tématu
author = "vy"
version = "1"
description = "Studené a klidné"

[colors]
background = "#001a2b"
accent = "#00a0ff"

[file_types]
"*.rs" = "#ffcc66"
"*.md" = "#66ccff"
```

Stačí uvést jen role, které chcete změnit — zbytek se zdědí z `base`, takže téma
nikdy neskončí s černým textem na černém pozadí. Barvy jsou ve tvaru `#rrggbb`;
přijímá se i zkrácený tvar `#rgb` a chybějící `#`. Neexistující název role je
chyba a v dialogu možností se ohlásí, místo aby se tiše ignoroval.

## Role

| Role | Kde se používá |
| --- | --- |
| `background` | pozadí panelů a oken |
| `foreground` | běžná jména souborů a text dialogů |
| `cursor_background`, `cursor_foreground` | řádek kurzoru v aktivním panelu |
| `selected_background`, `selected_foreground` | označené řádky |
| `directory` | názvy adresářů |
| `hidden` | skryté soubory a doplňkové popisky |
| `readonly` | soubory jen pro čtení a chybové hlášky |
| `archive` | archivy |
| `symlink` | symbolické odkazy |
| `header_background` | záhlaví sloupců, stavový řádek, lišta záložek |
| `panel_border` | rámečky panelů a oddělovače |
| `status_background` | pozadí stavového řádku |
| `accent` | rámeček aktivního panelu, ikony v liště |
| `row_alternate` | každý druhý řádek, aby dlouhé seznamy zůstaly čitelné |
| `warning` | nebezpečné akce a nezdařené kontroly |

## Dodávaná témata

* **Classic Light** — tradiční bílý seznam s tmavě modrým kurzorem.
* **Classic Dark** — stejné rozvržení na tmavém pozadí.
* **High Contrast** — černé pozadí, jasný text, výrazný kurzor; pro slabé displeje,
  projektory a slabozraké uživatele.

Obě klasická témata jsou zároveň vestavěný základ, takže aplikace nastartuje i bez
adresáře `themes/`.

## Přidání tématu

1. Zkopírujte existující soubor na `themes/<jméno>.toml` a změňte `[meta] name`.
2. Nastavte `base` na téma, ze kterého chcete vycházet, a vypište jen měněné role.
3. Použijte *Konfigurace ▸ Možnosti ▸ Barvy ▸ Načíst témata znovu*, nebo restartujte.

Témata se načítají při startu stejně jako `locales/` a `Docs/`; není potřeba nic
kompilovat a neznámý nebo poškozený soubor se ohlásí v dialogu možností, aniž by
zabránil načtení ostatních.

## Úpravy v aplikaci

Stránka Barvy vypíše všechny role s výběrem barvy a vedle ukazuje živý náhled panelu
s řádkem kurzoru, adresářem, skrytým a souborem jen pro čtení. Kontrast důležitých
dvojic se kontroluje průběžně: text na pozadí, text kurzoru na pozadí kurzoru, text
označení na pozadí označení a běžný text na střídavém řádku. Dvojice pod 3:1 se
ohlásí i s naměřeným poměrem, místo aby se tiše přijala.

*Uložit téma* zapíše soubor do adresáře témat a aktivuje ho. *Smazat téma* soubor
odstraní; vestavěná témata smazat nelze, takže cesta k čitelné barevné sadě existuje
vždy.

## Barvy podle typu souboru

`[file_types]` přiřazuje masce barvu, např. `"*.log" = "#808080"`. Syntaxe masek je
stejná jako všude jinde v aplikaci (upravují se v *Konfigurace ▸ Možnosti ▸
Zobrazení*). Když jméno odpovídá více maskám, vyhraje ta nejkonkrétnější; pravidlo z
aktivního tématu má přednost — žádný druhý zdroj barev neexistuje.

## Poznámky

* Soubor tématu je datový: smazání všech souborů témat obnoví vestavěné světlé téma a
  *Konfigurace ▸ Možnosti ▸ Nebezpečné ▸ Tovární nastavení* vymaže vybrané téma v
  rámci resetu nastavení.
* Jméno tématu v `settings.toml` je název souboru malými písmeny, takže přejmenování
  `name` uvnitř souboru neznamená ztrátu volby; nové jméno se prostě zobrazí v
  dialogu možností.
