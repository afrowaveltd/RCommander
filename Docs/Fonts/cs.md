# Písma

RCommander si nese dvě vlastní písma a každé z nich můžete nahradit jakýmkoli
řezem nainstalovaným v počítači. **Konfigurace → Možnosti → Písma**.

## Tři role

Text ve správci souborů není pořád stejný, proto jsou tři role místo jednoho
posuvníku velikosti:

| Role | Kde se používá | Výchozí |
| --- | --- | --- |
| **Text, seznamy a nabídky** | řádky souborů, nabídky, dialogy, hlášky | zabudované bezpatkové, 14 pt |
| **Nadpisy** | nadpisy dialogů a panelů | zabudované bezpatkové, větší |
| **Cesty a kód** | řádek s cestou, hex prohlížeč, editor, kód v nápovědě | zabudované nepoměrné, 14 pt |

Každá role má vlastní rodinu i velikost, takže široké nepoměrné písmo lze zmenšit,
aniž by se zmenšil seznam souborů. Blok **Náhled** pod voliči ukáže všechny tři
role s hodnotami, které se právě chystáte potvrdit.

## Odkud se bere nabídka

RCommander hledá soubory `ttf` a `otf` tam, kde je platforma má:

- Linux: `~/.local/share/fonts`, `~/.fonts`, `/usr/share/fonts`,
  `/usr/local/share/fonts`
- Windows: `%LOCALAPPDATA%\Microsoft\Windows\Fonts`, `%SystemRoot%\Fonts`
- macOS: `~/Library/Fonts`, `/Library/Fonts`, `/System/Library/Fonts`

Seznam se staví **jednou**, až když ho někdo potřebuje — start, který používá
zabudovaná písma, ho nikdy neskenuje. Počítač s tisíci souborů písem nemá
zdržovat správce souborů jen proto, že tahle funkce existuje. *Vyhledat znovu*
provede nové hledání; to potřebujete po instalaci písma za běhu RCommanderu.

## Poctivá omezení

- **Názvy rodin se čtou z názvů souborů.** Z `DejaVuSansMono-Bold.ttf` se stane
  *DejaVu Sans Mono* a tučný soubor se nenabízí jako samostatná rodina. Soubor,
  jehož název žádnou rozpoznatelnou rodinu nenese (`ab12cd.ttf`), se vůbec
  nezobrazí — nabízet „Ab12cd“ jako písmo by byla lež.
- **Kolekce písem (`.ttc`) se nenabízejí**: obsahují víc řezů a RCommander by za
  vás musel jeden vybrat.
- **Nabízejí se jen soubory, které začínají jako písmo.** Zbloudilý soubor s
  příponou `.ttf` se přeskočí, místo aby spadl první snímek, který vykreslí písmeno.
- Rodina, která byla vybraná a pak ze systému zmizela, se na stránce ohlásí
  („text, seznamy a nabídky: … nebylo nalezeno“) a použije se zabudovaný řez, takže
  okno zůstane čitelné a program nespadne při startu.
- **Ikony fungují dál.** Symboly v liště pocházejí ze zabudovaného písma, které
  zůstává posledním záložním řezem každé vybrané rodiny.

## V konfiguračním souboru

```toml
[fonts]
body_family = "Noto Sans"
heading_family = ""          # prázdné = rodina textu, o velikost větší
mono_family = "JetBrains Mono"
heading_size = 18.0
mono_size = 14.0
```

`display.font_size` zůstává velikostí základního textu, takže starší konfigurační
soubor funguje dál a nic se nemusí migrovat.
