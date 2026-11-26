# FTP připojení

Síťový panel promění jednu stranu okna na vzdálený souborový systém.

## Připojení

`Ctrl+N` otevře dialog *nové připojení*, `Ctrl+F` dialog rychlého připojení.
Vyplňte hostitele, port, uživatele a heslo, zvolte FTP nebo SFTP a případně
pasivní režim a výchozí adresáře. Připojení lze ukládat; seznam je součástí
vašeho konfiguračního souboru v části `[ftp] connections`.

`Ctrl+Shift+F` odpojí panel a vrátí ho na místní souborový systém.

## Uložené relace

`Ctrl+N` otevře dialog připojení, který má sekci *Uložené relace*. Připojení
vyplňte jednou, dejte mu jméno a stiskněte *Uložit*: profil se zapíše do
`sessions.toml` vedle konfigurace, odděleně od `settings.toml`, a při příštím
otevření dialogu se nabídne. Kliknutí na uloženou relaci vyplní formulář, *Načíst*
udělá totéž pro vybranou a *Smazat* ji odebere. Heslo není součástí profilu —
pochází z úložiště údajů, a relace, jejíž heslo ještě uložené není, dialog znovu
otevře s předvyplněným zbytkem.

Existující konfiguraci OpenSSH lze importovat: zadejte cestu (výchozí
`~/.ssh/config`) a stiskněte *Importovat ssh_config*. Každý konkrétní blok `Host`
se stane relací s využitím `HostName`, `User`, `Port` a `IdentityFile`; vzory jako
`Host *` nebo `Host 192.168.*` se přeskakují a existující jméno se naimportuje jako
`jméno 2`, takže se nic nepřepíše.

Akce s relacemi existují i jako volání modelu, takže MCP most i příkazová řádka
umí relace vypsat, uložit, přejmenovat, duplikovat, smazat a importovat bez
dialogu.

Potvrzení dialogu spustí připojení na pozadí: okno zůstává použitelné, stavový
řádek hlásí *Připojuji se k …* a panel se naplní adresářem serveru, jakmile je
spojení navázáno. Neodpovídající hostitel tedy stojí jedno hlášení, ne zamrzlé
okno.

## Práce se vzdáleným panelem

Vše funguje jako u místního adresáře: navigace, označování, kopírování oběma
směry, přejmenování, mazání, vytváření adresářů. Přenosy běží ve frontě na
pozadí, takže můžete dál procházet soubory, dokud velký upload neskončí.

## Pasivní a aktivní režim

Pasivní režim (výchozí) projde téměř každým firewallem a NAT a je doporučený.
Aktivní režim je k dispozici pro servery, které ho vyžadují. Podporuje-li server
`MLSD`, obsahuje výpis přesné časy a velikosti; jinak se analyzuje klasický
výpis `LIST` včetně symbolických odkazů a názvů s mezerami.

## Časy a synchronizace

Servery se liší ve vykazování časových pásem. RCommander považuje časy za UTC,
pokud server neuvádí jiný posun, a umí zachovat místní čas změny, je-li dostupný
přes `MDTM`. To je důležité pro *synchronizaci adresářů*, která časy porovnává.

## Bezpečnost

Hesla se do konfiguračního souboru ukládají jen na výslovné přání. Pro cokoli
jiného než důvěryhodnou místní síť používejte SFTP nebo FTPS a zkontrolujte
část `[ftp]` konfiguračního souboru, co je v něm uloženo.

## Kde jsou uložena hesla

Hesla a tokeny se **nikdy nezapisují do `settings.toml`**. Seznam připojení v tomto
souboru obsahuje jen názvy, hostitele, porty a uživatelská jména; heslo je uloženo v
úložišti pověření:

* v keyringu operačního systému, je-li k dispozici, jinak
* v šifrovaném souboru (`secrets.enc`) chráněném hlavní passphrase (odvození klíče
  Argon2id, šifrování AES-256-GCM, práva souboru jen pro vlastníka).

Starší konfigurační soubor, který ještě heslo obsahuje, se přečte jednou a provede
se migrace: hodnota se přesune do úložiště pověření a ze souboru se odstraní. Uložení
nastavení pak už heslo nikdy nemůže vyzradit, protože se toto pole vůbec
neserializuje.

Klíč úložiště se odvozuje z vaší passphrase při každém odemčení a neukládá se do
mezipaměti, takže zavřením aplikace se pověření uzamknou. Dialog nabídne *vytvoření*
hesla, když úložiště ještě neexistuje, a *odemčení*, když existuje. Zapomenuté heslo
lze nahradit tlačítkem *začít znovu* v tomto dialogu nebo položkou *Konfigurace →
Možnosti → Nebezpečné → Uložené přihlašovací údaje*, která úložiště smaže. Uložená
připojení (hostitel, uživatel, port) zůstanou, takže stačí znovu zadat hesla.

## SFTP a SSH

V dialogu připojení přepněte *protokol* na **SFTP**. SFTP vyžaduje uživatelské
jméno a nabízí tři způsoby ověření:

| Ověření | Co potřebuje |
| --- | --- |
| heslo | pole hesla, nebo hodnotu už uloženou v úložišti pověření |
| soubor s klíčem | cestu k privátnímu klíči; je-li klíč chráněný, aplikace se zeptá na passphrase. `~` se rozbaluje, takže `~/.ssh/id_ed25519` funguje |
| ssh-agent | jen běžícího agenta s alespoň jednou identitou |

### Klíče hostitele

Klíč hostitele se při každém připojení ověřuje a ukládá do `known_hosts` vedle
konfigurace:

* **trust on first use** (výchozí) — klíč se poprvé uloží a příště porovná,
* **strict** — neznámý hostitel je odmítnut, což je správná volba v řízené síti,
* **ignore** — žádné ověřování; jen pro zařízení, které si klíč generuje při každém
  startu.

**Změněný** klíč je odmítnut ve všech režimech kromě `ignore` a zpráva uvede oba klíče: změněný
klíč hostitele je přesně to, jak vypadá man-in-the-middle. Pokud víte, proč se klíč
serveru změnil, smažte jeho řádek z `known_hosts` a připojte se znovu.

### Když se připojení nepovede

Stavový řádek pokus vypráví: *hledám adresu hostitele*, *otevírám spojení*,
*vyměňuji klíče*, *ověřuji identitu serveru*, *přihlašuji*, *čtu adresář*. Krok, na
kterém se to zastaví, je odpověď na otázku „proč to selhalo":

| Zastavilo se na | Na co se podívat |
| --- | --- |
| hledání adresy | jméno hostitele a jestli vůbec funguje DNS |
| otevírání spojení | port, firewall, nebo server neběží |
| výměna klíčů | server mluví něčím, co toto sestavení neumí: zkuste jiný port nebo ověřte, že jde opravdu o SSH a ne FTP |
| identita serveru | klíč hostitele: `known_hosts`, nebo politika v dialogu |
| přihlašování | uživatel, heslo, soubor s klíčem nebo agent |
| čtení adresáře | účet se přihlásí, ale ten adresář mu nelze vypsat |

Dvě chyby jsou pojmenované přesně, protože dřív vypadaly stejně: *privátní klíč `…`
neexistuje* (s cestou, která se skutečně zkusila, po rozbalení `~`) a *server klíč `…`
odmítl*. Volba *ignorovat* u klíče hostitele neopraví ani jednu — týká se identity
serveru, ne té vaší.
