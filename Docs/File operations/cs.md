# Operace se soubory

## Kopírování a přesun

`F5` kopíruje, `F6` přesouvá. Oba příkazy otevřou dialog s cílem předvyplněným
z cílového panelu — můžete napsat jinou cestu. Volby dialogu:

- **zachovat časy** — ponechá původní čas změny,
- **ověřit** — znovu přečte zapsaná data a porovná je se zdrojem,
- **režim přepisu** — nahradit, přeskočit, automaticky přejmenovat nový soubor,
  nebo u každého existujícího souboru skončit chybou,
- **na pozadí** — přesun do fronty na pozadí, takže panely zůstanou použitelné.

Přesun v rámci jednoho souborového systému je přejmenování a je okamžitý; mezi
systémy se data zkopírují a zdroj se poté odstraní.

## Mazání

`F8` maže do koše platformy, pokud je k dispozici a je to v nastavení povoleno,
`Shift+F8` maže trvale. Oba příkazy se ptají na potvrzení, dokud potvrzování
nevypnete v nastavení.

## Vytváření a přejmenování

- `F7` vytvoří adresář ve zdrojovém panelu, `Shift+F7` v cílovém.
- `Shift+F6` přejmenuje na místě, `F6` přesune.
- Při kolizi a zvoleném přejmenování se automaticky vytvoří název s ` (2)`.

## Atributy a komentáře

*Změna atributů* přepíná příznaky jen pro čtení/skrytý/systém/archiv a na Linuxu
také unixová práva. `Ctrl+Z` upraví komentář souboru, který se ukládá do souboru
`descript.ion` v adresáři, a putuje tedy s vašimi soubory.

## Přerušení

Každá dlouhá operace zobrazuje průběh a lze ji přerušit; částečný výsledek
zůstane na disku a stavový řádek hlásí, co bylo dokončeno.

## Přenosy na pozadí

Zaškrtněte *na pozadí* v dialogu kopírování/přesunu (nebo si to nastavte jako
výchozí v *Konfigurace ▸ Možnosti*) a přenos poběží ve vlákně na pozadí: panely
zůstanou použitelné, stavový řádek ukazuje souhrnný průběh a právě zapisovaný
soubor, a tlačítko *Zrušit* úlohu zastaví. Každá úloha kontroluje token zrušení
mezi bloky, takže zastavení je okamžité a částečný výsledek zůstane na disku.
