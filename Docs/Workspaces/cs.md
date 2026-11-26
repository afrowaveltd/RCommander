# Práce s agentem na kopii

Klikněte pravým tlačítkem na soubor nebo složku a zvolte **Zpracovat pomocí
agenta…**: agent dostane **kopii** v pracovním prostoru a chat se otevře vedle
vašich souborů s otázkou, co s tím má udělat. Originálů se to nedotkne.

## Proč kopie

Protože model, který může rozbít jen kopii, je model, kterého si můžete dovolit
nechat běžet. Nejnižší úroveň oprávnění je proto ta *výchozí*: agent vidí soubory v

```
~/.config/rcommander/agent-workspaces/<čas>-<pid>/
```

a nic jiného. Co smí je věc oprávnění (viz *Konfigurace → Zabezpečení*); co vidí, je
ta složka.

## Co pracovní prostor obsahuje

- kopii každého vybraného souboru, složky včetně stromu;
- **manifest** (`workspace.toml`), který u každého souboru zaznamená původní cestu,
  kopii, SHA-256 originálu v okamžiku vytvoření a velikost. To je to, co později
  odpoví na „která verze je která" — a co řekne, jestli se kopie změnila, bez ptání
  agenta.

Soubor, který nelze přečíst, se v transkriptu jmenovitě ohlásí a přeskočí; jeden
nepřečtený soubor ve výběru vás nestojí ostatní. Pokud nešlo zkopírovat vůbec nic,
prostor se smaže a dostanete chybu místo prázdné konverzace.

## Konverzace

Transkript začíná poznámkou aplikace: které soubory agent má, kde kopie jsou a že
originály jsou nedotčené. Pak ve vstupu čeká otázka *„Co s tím mám udělat?"*, takže
další věc, kterou uděláte, je popis úkolu, ne vysvětlování situace.

Zavření pracovního prostoru z nabídky kopie smaže — to je bezpečný konec relace a je
to záměrně jedna položka daleko.

## Položky nabídky

| Položka | Co udělá |
| --- | --- |
| **Zpracovat pomocí agenta…** (`cm_ProcessWithAgent`) | připraví prostor, otevře chat, zeptá se co dál |
| **Zobrazit kopii agenta** (`cm_OpenWorkspace`) | ukáže složku pracovního prostoru v panelu, ať vidíte přesně to, co agent |
| **Zavřít pracovní prostor agenta** (`cm_DiscardWorkspace`) | smaže kopie |

## Co tu ještě není

**Posílání změněných kopií zpět do originálů.** To je review pipeline z fáze 16.4 —
diff, schválení po souborech, transakce s ověřenou zálohou, záznam v auditu — a je to
záměrně samostatný, vědomý krok. Do té doby je pracovní prostor místem, kde se
*díváte a přemýšlíte*, ne kde se publikuje; co chcete přenést, přenesete sami, svýma
rukama a svým označením.
