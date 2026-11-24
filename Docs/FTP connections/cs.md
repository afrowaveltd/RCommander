# FTP připojení

Síťový panel promění jednu stranu okna na vzdálený souborový systém.

## Připojení

`Ctrl+N` otevře dialog *nové připojení*, `Ctrl+F` dialog rychlého připojení.
Vyplňte hostitele, port, uživatele a heslo, zvolte FTP nebo SFTP a případně
pasivní režim a výchozí adresáře. Připojení lze ukládat; seznam je součástí
vašeho konfiguračního souboru v části `[ftp] connections`.

`Ctrl+Shift+F` odpojí panel a vrátí ho na místní souborový systém.

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
mezipaměti, takže zavřením aplikace se pověření uzamknou. Pokud passphrase ztratíte,
jsou hesla nedobytná — uložená připojení (hostitel, uživatel, port) zůstanou a
stačí znovu zadat hesla.
