# Souborové utility

Tyto nástroje najdete v nabídce *Soubor* a pracují s označenými soubory, případně
se souborem pod kurzorem, pokud nic označeno není.

## Rozdělení a sloučení

*Rozdělit soubor* rozseká soubor na číslované části (`report.001`, `report.002`, …)
nastavitelné velikosti — klasický způsob, jak dostat velký soubor na médium nebo
ho poslat po částech. *Sloučit soubory* části zase poskládá v číselném pořadí do
nového souboru.

Číslování se čte od `.001` výš a zastaví se na první mezeře, takže neúplná sada je
odhalena, místo aby tiše vzniklo zkrácené pokračování. Zdrojový soubor ani jedna
operace nemění a obě respektují tlačítko pro zrušení.

## Kontrolní součty

*Vytvořit soubor(y) kontrolních součtů* zapíše seznam hashů vedle souborů.
Podporované algoritmy jsou **CRC32** (`.sfv`), **MD5** (`.md5`), **SHA1**
(`.sha1`) a **SHA256** (`.sha256`), přípona souboru odpovídá algoritmu.

Zápis používá konvenci `md5sum`/`sha256sum`, `<hash>  <název>`, s cestami
relativními k aktuálnímu adresáři, takže seznam lze ověřit z libovolného místa pod
ním. *Ověřit kontrolní součty* takový soubor přečte a u každého souboru oznámí,
zda obsah stále odpovídá.

Čtečka přijímá i starší konvenci **SFV** (`<název> <hash>`), takže seznamy
z jiných nástrojů se ověří bez konverze. Komentáře (`#`, `;`) a prázdné řádky se
přeskakují a chybějící soubor se hlásí jako chyba, ne jako tiché přeskočení.

## Porovnání podle obsahu

*Porovnat podle obsahu* porovná dva soubory bajt po bajtu a oznámí buď že jsou
shodné, nebo offset prvního rozdílu. Pro adresáře použijte *Porovnat adresáře*
(`Shift+F2`), které srovnává názvy, velikosti a časy, nebo dialog synchronizace
s volbou *porovnávat obsah* pro kontrolu na úrovni bajtů.

## Hledání duplicit

Hledač duplicit prochází strom ve dvou krocích: soubory nejprve seskupí podle
velikosti a hashuje jen ty, které velikost sdílejí. Složka s tisíci unikátních
souborů tedy nestojí téměř nic, zatímco skutečné duplicity potvrdí hash.

Výsledkem je seznam skupin seřazený podle zbytečně obsazeného místa s plnými
cestami všech kopií. Nic se nemaže automaticky — rozhodujete vy a označení lze
dál použít s `F5`, `F8` nebo hromadným přejmenováním.

Skryté a velmi malé soubory se ve výchozím stavu přeskakují; obojí lze v dialogu
změnit.

## Kódování a dekódování

*Zakódovat soubor* převede binární soubor na tisknutelný text a *Dekódovat
soubor* ho převede zpět. Podporována jsou tři kódování:

| Kódování | Abeceda | Typické použití |
| --- | --- | --- |
| MIME | base64, `A–Z a–z 0–9 + /` | přílohy e-mailů, moderní výchozí volba |
| UUE | klasická sada UUencode začínající mezerou | soubory ze starších unixových nástrojů |
| XXE | `+ - 0–9 A–Z a–z` | varianty, které se vyhýbají znakům problematickým pro shell |

Výstup si zachovává klasickou obálku (`begin 644 <název>` … `end`) a zalamuje se po
60 znacích (base64) nebo po 45 bajtech na řádek (UU/XX). Při dekódování není třeba
kódování znát: obsah s konzistentními délkami řádků se čte jako UU/XX, cokoli
jiného jako base64, a název souboru z obálky se obnoví — takže z `report.pdf.b64`
je zase `report.pdf`.

## Branch view

*Příkazy → Branch View* (`Ctrl+B`) nahradí obsah panelu plochým výpisem všeho pod
aktuálním adresářem s cestami místo názvů (`src/main.rs`). Celý podstrom se stane
jedním řaditelným, filtrovatelným a označitelným seznamem — hodí se k nalezení
všech souborů určitého typu, ke kopírování výběru z mnoha složek najednou nebo ke
kontrole, co složka skutečně obsahuje. Opětovné spuštění příkazu vrátí normální
výpis.

## Uložené výběry

Nabídka *Označit* umí aktuální výběr uložit a později obnovit (`Ctrl+Num /`
obnoví poslední), nebo ho zapsat do souboru v aktuálním adresáři a zase načíst —
praktické pro uchování zajímavých souborů mezi sezeními nebo pro předání skriptu.
Položky, které už neexistují, se při obnovení odstraní.
