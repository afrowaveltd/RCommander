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

## Posílání změn zpět

**Poslat změny zpět…** (`cm_ReviewWorkspace`, ve stejné kontextové nabídce) otevře
revizi: každý soubor v pracovním prostoru s tím, co se s ním stalo, a u toho
kliknutého jeho **diff**.

| Co vidíte | Co to znamená |
| --- | --- |
| `+ řádek` | kopie agenta tento řádek přidává |
| `- řádek` | kopie agenta ho odebírá |
| `… 20 řádků beze změny` | kontext je sbalený; nic, co se změnilo, se neskrylo |
| `binární` | není to text, takže diff není možný — a nikdy se to nezapíše omylem |
| varování u řádku | originál se po vytvoření prostoru změnil |

Zapisují se jen **zaškrtnuté** soubory, a předzaškrtnuté jsou jen ty, které se
skutečně změnily. Bezpečnost stojí na třech pravidlech:

1. **Nezapíše se nic, co jste nezaškrtli**, soubor po souboru.
2. **Zastaralý originál se odmítne.** Když se soubor mezi „zpracovat pomocí agenta" a
   „poslat změny zpět" změnil, kopie už není změnou *toho* souboru — a přepsat tu
   novější verzi by byla nejhorší věc, kterou tahle funkce může udělat.
3. **Každý zápis jde přes transakční vrstvu**: nejdřív ověřená záloha, pak zápis, pak
   přehashování. Zálohy končí v
   `~/.config/rcommander/agent-workspaces/apply-<čas>/` vedle manifestu, takže
   schválená změna je vratná a přerušená se obnoví.

Každý zápis se také zapíše do auditu s důvodem *approved in the workspace review* —
změna s vedlejším účinkem nikdy není tichá.

Potom je pracovní prostor zase konzistentní: kopie se rovnají originálům, což říká i
manifest.

## Co ještě chybí

Revize je po celých souborech. Třířádková změna v tisíciřádkovém souboru se ukáže jako
třířádkový diff, ale **schvalování po hunkech** tu ještě není — a stejně tak spouštění
validátoru (JSON/TOML parse, `cargo check`) nad výstupem agenta před zobrazením revize.
Obojí patří ke zbytku fáze 16.4 a na bezpečnostních pravidlech výše nic nemění.
