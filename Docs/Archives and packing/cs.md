# Archivy a balení

Archivy se chovají jako adresáře: `Enter` na souboru `.zip`, `.tar.gz` nebo `.7z`
ho otevře a panel vypíše jeho obsah. `Backspace` archiv zase opustí.

## Vytvoření archivu

`Alt+F5` otevře dialog zabalení pro označené soubory (nebo soubor pod kurzorem).
Zvolte formát — vytvářet lze ZIP a TAR.GZ — a úroveň komprese od 0 (bez
komprese) do 9 (maximum). Archiv se zapisuje atomicky: objeví se, až když je
hotový, takže přerušená operace nikdy nenechá napůl zapsaný soubor.

## Rozbalení

`Alt+F6` rozbalí vybraný archiv (nebo označené položky archivu, ve kterém jste)
do cílového panelu. Cesty uvnitř archivu se vytvoří pod cílovým adresářem;
položky, které by z cíle utekly (`..`, absolutní cesty), jsou odmítnuty.

## Podporované formáty

| Formát | Čtení | Vytváření |
| --- | --- | --- |
| ZIP | ano | ano |
| TAR, TAR.GZ, TAR.BZ2, TAR.XZ, TAR.ZST | ano | TAR.GZ |
| GZ, BZ2, XZ, ZST (jeden soubor) | ano | ne |
| 7Z | ano (je-li backend zapnut) | ne |
| RAR, CAB | jen rozpoznání a popis | ne |

`Alt+Shift+F9` otestuje archiv na poškození bez rozbalení.
