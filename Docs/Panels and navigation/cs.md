# Panely a navigace

## Dva panely

Aktivní panel kreslí kurzor převráceně. Stavový řádek aktivního panelu ukazuje,
kolik obsahuje souborů a adresářů, jejich celkovou velikost a při označení také
počet a velikost označených položek.

## Navigace

| Klávesa | Akce |
| --- | --- |
| `Enter` | otevře adresář / archiv pod kurzorem |
| `Backspace`, `Ctrl+PgUp` | přejde do nadřazeného adresáře |
| `Alt+Left`, `Alt+Right` | předchozí / následující adresář z historie |
| `Alt+F1`, `Alt+F2` | výběr disku či svazku pro levý / pravý panel |
| `Ctrl+R`, `F2` | znovu načte adresář |
| `Ctrl+Left/Right` | zobrazí adresář pod kurzorem v druhém panelu |
| `Tab` | přepne aktivní panel |
| `Ctrl+U` | prohodí oba panely |
| `Ctrl+Shift+U` | nastaví cílový panel na adresář zdrojového panelu |

## Režimy zobrazení

| Klávesa | Režim |
| --- | --- |
| `Ctrl+F1` | stručný — jen názvy, zalamované do sloupců |
| `Ctrl+F2` | podrobný — název, velikost, datum, atributy |
| `Ctrl+Shift+F2` | podrobný včetně komentáře souboru |
| `Ctrl+Shift+F1` | náhledy |
| `Ctrl+Q` | panel rychlého náhledu |

## Řazení

`Ctrl+F3` název, `Ctrl+F4` přípona, `Ctrl+F5` čas, `Ctrl+F6` velikost,
`Ctrl+F7` neřazeno. Druhým stiskem stejné klávesy se pořadí obrátí. Názvy se
řadí přirozeně, takže `file2` je před `file10`.

## Filtrování

Maska v panelu (`*.txt;*.md`) skryje soubory, které jí neodpovídají; adresáře
zůstávají vždy viditelné. `Ctrl+F10` opět zobrazí všechny soubory.

## Rychlý náhled a strom

`Ctrl+Q` nahradí neaktivní panel rychlým náhledem souboru pod kurzorem: obrázky
se zobrazí jako obrázky, text jako text (s rozpoznaným kódováním a podporou
zprava doleva) a neznámé binární soubory jako hexdump. Dalším `Ctrl+Q` se vrátí
seznam souborů.

`Ctrl+Shift+F8` zobrazí místo rychlého náhledu strom adresářů, což výrazně
zrychlí pohyb v hluboké hierarchii složek.

## Výběr disku

Na začátku cesty každého panelu je **výběr svazku**. Ve Windows vypíše všechny disky jako
`C: Systém`, `D: Data`, nebo jen `C:`, když disk nemá jméno. V Linuxu vypíše jen souborové
systémy, které uživatel opravdu použije — odpojitelná média, síťové share, optické disky — a
vynechá systémový kořen, obrazy `snap` a pseudosystémové systémy, protože seznam plný
`/snap/core22/3129` nikomu nepomůže. Kořen zůstává dostupný napsáním `/` do cesty.

Jména pocházejí ze samotného svazku (`/dev/disk/by-label` v Linuxu, informace o svazku ve
Windows), jinak se zobrazí přípojný bod. U každé položky je volné místo (`DATA — 220 GB`) a
odpojitelná média jsou nahoře, protože disk, který jsi právě připojil, je ten, který hledáš.

## Nabídka pravého tlačítka

Pravý klik na řádek ho označí (jako v Průzkumníku) a otevře nabídku operací, které tam dávají
smysl: u adresáře *Otevřít*, u souboru *Zobrazit* a *Upravit*, dále *Kopírovat*, *Přesunout*,
*Nový adresář*, *Přejmenovat*, *Smazat* a *Atributy*. U každé položky je klávesová zkratka, takže
kdo ji nezná, naučí se ji používáním nabídky.

Pravý klik do **prázdné plochy** pod řádky (nebo na rámeček panelu) otevře nabídku pro samotný
adresář: *Nový adresář*, *Obnovit*, *Nadřazený adresář*, *Označit vše* a položky úložiště
přihlašovacích údajů a konfigurace. Klik zároveň aktivuje panel, takže nabídka vždy platí pro ten
panel, na který jsi ukázal.

Až bude vrstva agenta, patří sem i věci typu „zkontrolovat agentem" nebo „publikovat na server" —
nabídka je pro ně přirozené místo.

## Otevírání souborů

`Enter` a **dvojklik** znamenají *výchozí akci vašeho systému*: program se spustí,
cokoli jiného se otevře v aplikaci, kterou tomu desktop přiřadil, a stavový řádek
řekne, co se stalo. Zabudovaný prohlížeč zůstává na `F3`, takže podívat se na textový
soubor nikdy neznamená obejít jiný program.

Kontextová nabídka přidává akce, které se od správce souborů čekají:

| Položka | Co udělá |
| --- | --- |
| **Otevřít (přiřazená aplikace)** | totéž co Enter |
| **Otevřít v aplikaci…** | na Windows nativní volič; jinde prohledatelný seznam aplikací, které desktop zná (čtený ze souborů `.desktop`), a to jen těch, které se mají zobrazovat |
| **Odeslat e-mailem…** | přiloží soubor k nové zprávě: `xdg-email`, Mail na macOS, výchozí handler na Windows |
| **Spustit v terminálu** | spustí program v terminálu, takže jeho výstup zůstane na obrazovce |
| **Otevřít terminál zde** | terminál ve složce panelu, s tím emulátorem, který máte (`$TERMINAL`, `gnome-terminal`, `konsole`, `xfce4-terminal`, `alacritty`, `kitty`, `xterm`) |

Nic nejde přes příkazovou řádku shellu, takže soubor jménem `; rm -rf ~` je název
souboru a ne instrukce.

## Panely, které nejsou adresáře

Panel nemusí zobrazovat souborový systém. Plugin může poskytnout **zdroj** — poštovní schránku,
katalog fotografií, seznam ticketů — a panel ho vykreslí stejně jako složku, protože se pořád ptá
jen na dvě věci: *co v tomto umístění je* a *co udělá tento řádek*.

Mění se to, co zdroj deklaruje:

| Zdroj deklaruje | Co uvidíte |
| --- | --- |
| **sloupce** | vlastní hlavičky (*Předmět, Priorita, Stáří*), které v tomto panelu nahradí nastavené sloupce souborů. Druhý panel si nechá svoje |
| **akce řádku** | další položky dole v místní nabídce — *Odpovědět*, *Vyvolat*, *Přiřadit mně*. Provádí je zdroj; RCommander nehádá, co znamenají |
| **schopnosti** | které souborové operace tu mají smysl. To je ta důležitá část |

Schopnosti se odpovídají záporně: zdroj, který neřekne, že podporuje mazání, **nemůže mít smazané
řádky**, a funkční klávesa se vykreslí jako nedostupná, místo aby se potichu tvářila, že jde
použít. Neotevře se žádný dialog, nic se neschvaluje a nic nepředstírá, že to skoro vyšlo.
Kopírování, přesun, vytvoření, smazání, přejmenování a zobrazení se deklarují jednotlivě; zdroj,
který nedeklaruje nic, je seznam, který si můžete jen přečíst.

To není oprávnění — je to podoba místa. I tam, kde zdroj mazání *deklaruje*, rozhoduje politika,
schvalovací dialog a audit stejně jako u místního souboru.

Panel, jehož zdroj je nedostupný, to napíše do panelu. Nikdy neukáže prázdný seznam, protože
„server neodpověděl" a „tato složka je prázdná" nesmí vypadat stejně.
