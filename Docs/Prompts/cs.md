# Prompty

Prompt je uložená otázka. RCommander přináší malý katalog rozdělený podle tématu a
vy si můžete přidat vlastní. Picker otevřete tlačítkem **Prompty…** ve vstupu chatu.

## Prompt jsou data, ne oprávnění

Na téhle funkci je nejdůležitější to, co **nedělá**. Vložení promptu vloží text do
vstupu chatu. Nic nepovolí:

- co agent smí, rozhodují oprávnění (viz *Konfigurace → Zabezpečení*), ne nic
  napsané v promptu;
- prompt, který si „řekne" o víc přístupu, se odmítne stejně jako ten, co nežádá nic;
- proměnné se plní z toho, co aplikace už ví, a **schválně neexistuje** proměnná pro
  *obsah* souboru — prompt tedy nemůže potichu někam poslat data.

Proto jsou prompty obyčejné soubory TOML, které si můžete přečíst, zkopírovat,
porovnat a dát do repozitáře.

## Katalog

| Kategorie | K čemu je |
| --- | --- |
| **Soubory** | vysvětlit, shrnout, porovnat, přeložit, navrhnout bezpečnou úpravu |
| **Git** | zprávy k commitům, revize diffu, „proč to přestalo fungovat", hygiena commitu |
| **Vzdálené** | porovnání lokálního a vzdáleného, plán nahrání, práva, selhaný přenos |
| **Build** | selhaný build, manifest, závislost, plán upgradu |
| **Diagnostika** | čtení logu, služba, která nenaběhne, plný disk, „je to pomalé" |
| **Obecné** | co agent vidí, plán před akcí, kritická revize, vysvětlení |

Zabudované prompty jsou **jen pro čtení**: upgrade ten soubor nahradí, takže v něm
nikdy nesmí být vaše úpravy. Chcete-li ho změnit, zkopírujte si ho — kopie je vaše a
upgrady na ni nesahají.

## Proměnné

Tělo promptu může používat `{{name}}`. Tyto se plní automaticky:

| Proměnná | Hodnota |
| --- | --- |
| `{{cwd}}` | adresář aktivního panelu |
| `{{selection}}` | soubor nebo složka pod kurzorem |
| `{{selection_count}}` | kolik položek je označeno, pokud je jich víc |
| `{{model}}` | model, který chat použije |
| `{{host}}` | hostitel, když panel ukazuje vzdálené umístění |

Picker ukáže hotový text **před** vložením, s proměnnými už vyplněnými. Proměnná,
kterou nelze vyplnit, zůstane viditelná jako `{{name}}` a náhled řekne která to je:
prázdné místo by byla tichá chyba, viditelná značka je poctivá.

## Vlastní prompty

Prompty bydlí vedle ostatní konfigurace:

```
~/.config/rcommander/prompts/git.toml        # jeden soubor na kategorii
```

```toml
[meta]
name = "Moje gitové prompty"
category = "git"

[[prompt]]
id = "git.my-message"
title = "Zpráva k commitu naším stylem"
about = "Krátký předmět, žádný výčet souborů"
text = "Napiš zprávu ke commitu v {{cwd}} naším stylem: krátký předmět, pak proč to bylo důležité."
```

Soubor uložte a picker znovu otevřete; nic se nerestartuje. Položka bez názvu
použije své id, prompt bez id se ohlásí a soubor s překlepem vás stojí jen ten jeden
soubor — zbytek katalogu se načte.

## Co tu ještě není

Katalog se čte a prohledává; editor, který by umožnil přidat prompt bez sahání do
textového souboru, je další krok (*Konfigurace → Prompty*), spolu s resetem a diffem
pro upravený systémový prompt. Do té doby **je** editor ten soubor TOML, což není
špatné místo: dá se verzovat, porovnávat a přenášet.
