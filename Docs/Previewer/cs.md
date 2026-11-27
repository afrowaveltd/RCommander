# Prohlížeč (F3)

`F3` otevře soubor pod kurzorem ve vestavěném prohlížeči. Prohlížeč rozpozná, co
soubor *je*, podle prvních bajtů, nejen podle přípony, a zvolí nejlepší
zobrazení:

| Obsah | Jak se zobrazí |
| --- | --- |
| obrázky (PNG, JPEG, GIF, BMP, WebP) | dekóduje se a zmenší do okna |
| text (libovolné kódování) | dekóduje se s automatickou detekcí kódování, ohled na RTL |
| Markdown | vykreslený společným rendererem chatu a nápovědy; Zdroj ukáže původní text |
| HTML, JSON, XML, CSV | zdrojový text; vlastní renderer zatím není k dispozici |
| archivy, spustitelné soubory, databáze, média | popis ve stavovém řádku, data dostupná v hexu |
| cokoli neznámého či binárního | integrovaný hexdump |

## Režimy

Na liště zvolte **Náhled**, **Zdroj**, **Hex** nebo **Obrázek**. Markdown se parsuje jednou a rozsáhlé dokumenty mají označený omezený náhled. Odkazy zobrazí svůj cíl bez jeho otevření. F4 upravuje původní zdroj se zvýrazněním syntaxe.

### Hexdump

Dump ukazuje offset, bajty ve skupinách a sloupec tisknutelných ASCII znaků — a je
určený ke *čtení*, nejen k podívání:

- **šířku si určujete sami**: posuvník *bajtů/řádek* dump okamžitě přeformátuje, od
  1 do 64 bajtů na řádek, bez znovunačtení souboru;
- **skroluje pořádně** oběma směry a kreslí jen viditelné řádky, takže
  několika-megabajtový soubor není pomalejší než malý;
- **kliknutím vyberete řádek**: pruh dole ukáže offset v hexu i desítkově a bajty
  daného řádku jako unsigned, signed, 16bitové a 32bitové číslo (little endian, jak to
  x86 formáty souborů myslí), plus ASCII;
- **přejít na** bere `0x200`, `512`, `200h` i holý hex a skočí tam — tak se sleduje
  hlavička, která říká „tabulka začíná na 0x200“.

Soubor, který není text, se otevře rovnou tady: není co dekódovat a předstírat opak
by ukázalo mojibake.

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


## Umístění v pracovním prostoru

Výchozí zobrazení překryje oba panely. Ovládací prvky umožňují přesun do
levého či pravého panelu nebo do plovoucího okna uvnitř hlavní aplikace.
Přesun nemění otevřený soubor ani rozepsaný text. Druhý panel zůstává
použitelný; kliknutím jej aktivujete. Samostatné okno operačního systému
ani integrovaný terminál zatím nejsou součástí této změny.
