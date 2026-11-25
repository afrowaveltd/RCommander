# Konfigurace

*Konfigurace ▸ Možnosti* upravuje vše; změny se projevují živě a do
`settings.toml` se zapíší po potvrzení.

| Sekce | Co ovládá |
| --- | --- |
| Zobrazení | režim zobrazení, skryté soubory, položku `..`, velikost písma, výšku řádku, ikony, náhledy, přirozené řazení |
| Jazyk | jazyk rozhraní, náhled směru |
| Barvy | světlý/tmavý motiv, barvy kurzoru a označení, barvy podle typu souboru |
| Písma | tři role textu — základní, nadpisy a nepoměrné — každá s vlastní rodinou a velikostí |
| Editor | vestavěný editor nebo externí příkaz, písmo, šířka tabulátoru, konce řádků, zalamování |
| Archivy | výchozí formát archivu, úroveň komprese, přípony archivů |
| FTP | pasivní režim, MLSD, kódování, časové limity, uložená připojení |
| Agent | zapnutí MCP serveru, přenos, port, token, politika potvrzování, MCP klienti |
| Zabezpečení | oprávnění agenta, které operace se vždy ptají, trvale povolené operace, tlačítko Zastavit všechno a auditní log |
| Zkratky | celá klávesová mapa, upravitelná řádek po řádku |

## Konfigurační soubor

`settings.toml` je určen i k ruční úpravě. Neznámé klíče se zachovají, takže se
soubor z novější verze stále načte. Soubor vznikne při prvním spuštění a přepíše
se po volbě *Konfigurace ▸ Uložit nastavení*.

## Zabezpečení

Stránka *Zabezpečení* rozhoduje, co agent smí. Platí jen pro agenta: vaše vlastní
klikání v panelech se jí neřídí, protože správce souborů, který by si sám sebe
ptal na povolení, by se nedal používat.

### Dialog schválení

Když žádost potřebuje schválení — git push od agenta, mazání mimo sdílenou
složku, akce se zvýšenými právy — objeví se dialog, který pojmenuje původce,
operaci, cíl a kam data půjdou, a pak vypíše **co souhlas znamená** lidsky
(„Data opustí tento počítač. Vzdálený server si kopii ponechá, i když tu svou
smažete.“). Nabídne tři odpovědi:

| Volba | Co se stane |
| --- | --- |
| **Povolit nyní** | tato jedna žádost proběhne; příští se zase zeptá |
| **Povolit v této relaci** | všechny žádosti této operace běží bez ptaní až do ukončení programu nebo do stisku *Zastavit všechno hned* |
| **Povolit vždy…** | otevře se **druhé okno** s přesným rozsahem, upozorněním, že se rozhodnutí zapíše do `settings.toml`, a políčkem, kam musíte napsat `yes`, než jde *Odeslat* stisknout |

Napsané slovo je schválně zpomalovací práh: klik může být omyl, napsané `yes`
ne. Není to divadlo — je to rozdíl mezi jedním a dvěma dialogy a hlavně to dělá
cestu zpět snadno najitelnou, protože stejná stránka vypisuje každé trvalé
povolení s tlačítkem *Vzít zpět*.

### Co trvalé povolení neumí

- Neodemkne cestu na **seznamu zákazů** (`~/.ssh`, vnitřnosti `.git`, …).
- Nerozšíří profil: politika `sandbox` akce se zvýšenými právy vůbec nenabízí,
  takže je nemůže ani předem schválit.
- Neschová se. Každé použití zapíše do auditu grant, který ho povolil, a hashový
  řetěz logu ukáže, jestli se do něj zasahovalo.

### Stránky

- **Kdo agent je** — profil oprávnění (`sandbox`, `user`, `privileged`), každý
  s jednou poctivou větou.
- **Vždy se zeptat před** — operace, které otevřou dialog i tehdy, když je
  pokrývá grant. Nezaškrtnout nic neznamená „nic se neptej“: bezpečný výchozí
  seznam (`execute`, `publish`, `privileged`) se vrátí.
- **Povoleno trvale** — uložená předběžná schválení s datem a možností odebrání.
- **Povoleno pro tuto relaci** — co současný běh povolil, s tlačítkem *Zastavit
  všechno hned*, které to vyčistí i zruší granty relace.
- **Auditní log** — kolik rozhodnutí je zaznamenaných a jestli je hashový řetěz
  v pořádku; poslední řádek je vidět, takže je hned jasné, co se právě stalo.

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
