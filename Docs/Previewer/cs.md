# Prohlížeč (F3)

`F3` otevře soubor pod kurzorem ve vestavěném prohlížeči. Prohlížeč rozpozná, co
soubor *je*, podle prvních bajtů, nejen podle přípony, a zvolí nejlepší
zobrazení:

| Obsah | Jak se zobrazí |
| --- | --- |
| obrázky (PNG, JPEG, GIF, BMP, WebP) | dekóduje se a zmenší do okna |
| text (libovolné kódování) | dekóduje se s automatickou detekcí kódování, ohled na RTL |
| HTML, Markdown, JSON, XML, CSV | jako text, formát je uveden ve stavovém řádku |
| archivy, spustitelné soubory, databáze, média | popis ve stavovém řádku, data dostupná v hexu |
| cokoli neznámého či binárního | integrovaný hexdump |

## Režimy

`T` text, `H` hex, `I` obrázek, `A` automaticky. Hexdump zobrazuje offset, bajty
ve skupinách a sloupec tisknutelných ASCII znaků; počet bajtů na řádek je
nastavitelný.

## Klávesy

| Klávesa | Akce |
| --- | --- |
| `F3` / `Enter` | otevře další soubor |
| `F2` | znovu načte soubor |
| `T`, `H`, `I`, `A` | přepne režim |
| `F7`, `Ctrl+F` | hledat text |
| `F3`, `Shift+F7` | hledat další |
| `+`, `-` | zvětšit / zmenšit |
| `Ctrl+0` | obnovit měřítko |
| `F` | přizpůsobit obrázek oknu |
| `W` | zapnout/vypnout zalamování řádků |
| `Esc`, `Alt+F4` | zavřít okno |

## Velké soubory

Pro náhled textu a hexu se načítá jen omezené množství dat (výchozí 8 MiB).
Stavový řádek upozorní, je-li náhled zkrácen, a vždy ukazuje přesnou velikost
souboru. Obrázky se dekódují s limitem pixelů, takže obrázek o 200 megapixelech
nemůže vyčerpat paměť — místo toho se pro zobrazení zmenší.

## Orientace

Je-li text arabský, hebrejský, perský nebo urdský, přepne se prohlížeč
automaticky na tok zprava doleva. Zarovnání lze ručně přepnout pomocí
`Ctrl+Right Shift` a `Ctrl+Left Shift`.
