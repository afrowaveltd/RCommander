# Vlastní sloupce a režimy zobrazení

## Sady sloupců

Kromě vestavěného *stručného* a *podrobného* zobrazení si můžete definovat vlastní
sady sloupců: název, přípona, velikost, datum, atributy, unixová práva, vlastník,
komentář a hodnoty poskytované obsahovými nástroji MCP. Sady mají názvy a vybírají
se v nabídce zobrazení (`Shift+F1`).

## Režimy zobrazení

Režim zobrazení spojuje sadu sloupců, řazení, filtr a barvy, takže jediný stisk
klávesy přepne celý panel například na „jen obrázky, řazené podle data, zobrazené
jako náhledy“. Režimy se ukládají do konfiguračního souboru a lze je přepínat
automaticky při vstupu do adresáře, jehož název odpovídá pravidlu.

## Náhledy

`Ctrl+Shift+F1` zobrazí mřížku náhledů. Náhledy se kešují na disku; velikost
i umístění keše jsou nastavitelné. Velké obrázky se jednou zmenší a znovu
použijí, takže procházení složky s fotografiemi zůstává rychlé.

## Barvy podle typu souboru

Konkrétním maskám lze přiřadit vlastní barvu — hodí se to k zešednutí `*.log`,
zčervenání `*.bak` nebo zeleným archivům. Pravidla se vyhodnocují v pořadí, první
shoda rozhoduje.

## Náhledy

`Ctrl+Shift+F1` přepne panel na mřížku náhledů. Obrázky se dekódují s limitem
pixelů a jednou zmenší, pak se kešují jako GPU textury podle cesty a velikosti,
takže procházení složky s tisíci fotografiemi nic nedekóduje dvakrát. Keš je
omezená (512 náhledů, nejdéle nepoužité se zahazují), takže paměť zůstává
konstantní.

Adresáře se zobrazují jako `[DIR]`, vše, co není dekódovatelný obrázek, se
zobrazí podle přípony, takže mřížka se nikdy nezasekne na obřím nebo poškozeném
souboru. Velikost náhledu je nastavitelná (*Konfigurace ▸ Možnosti ▸ Zobrazení*).
