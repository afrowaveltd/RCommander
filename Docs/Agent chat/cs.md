# Chat s agentem

Chat je rozhovor s jazykovým modelem přímo ve správci souborů, takže se můžete
zeptat na to, na co se právě díváte, aniž byste přepínali okna. Otevřete ho v
**Konfigurace → Chat s agentem** nebo příkazem `cm_AiChat`.

## Co umí

- Odpověď **proudí**: text se objevuje už během generování. Tlačítko *Zastavit*
  zachová, co už dorazilo, místo aby text zahodilo.
- Transkript je zároveň historie dotazu, takže doplňující otázka („a ten druhý?“)
  vidí všechno, co bylo řečeno před ní.
- *Nový rozhovor* transkript vymaže a začne od začátku.
- `Enter` odešle, `Shift+Enter` přidá řádek — víceřádkové otázky tedy fungují.

## Modely

Chat používá model vybraný v nabídce modelů (viz téma *Konfigurace*). RCommander
má zabudovanou podporu **Ollamy**, takže výchozí je místní model a nic neopouští
počítač. Poskytovatel, adresa a model jsou v souboru nastavení:

```toml
[agent]
provider_kind = "ollama"
provider_url = "http://127.0.0.1:11434"
model = "qwen3:8b"
```

Když není vybraný žádný model, chat to řekne místo tichého selhání: otevřete
nabídku modelů, stiskněte *Znovu vyhledat modely* a nějaký vyberte.

## Co chat neumí

Rozhovor **nemá přístup k souborům**. Nemůže nic číst, zapisovat, kopírovat ani
mazat. To omezení je záměrné: každá akce s vedlejším účinkem patří do oprávnění
(policy), která si vyžádají potvrzení a zapíší se do auditu. Nástroje, které
skutečně mění data, jsou MCP nástroje agenta a procházejí stejnou branou —
odpověď v chatu může akci navrhnout, ale její provedení vždy projde touto branou.

Zastavení modelu i zavření okna jsou vždy bezpečné: nedokončená odpověď ztratí
jen zbytek textu, nikdy data.

## Známá omezení

- Chat sám nespouští nástroje; volání nástrojů potřebuje model, který to umí, a v
  tomto okně zatím zapojené není.
- Odpovědi se vykreslují jako Markdown — nadpisy, seznamy, tabulky, citace,
  řádkový kód i bloky kódu — ale kód zatím **není** zvýrazněný syntaxí a odkazy
  v odpovědi se kreslí jako text, nikoli otevírají.
- Transkript je jen v paměti. Zavřením RCommanderu rozhovor skončí; místní
  úložiště je v plánu.
- Nedokončená odpověď se vykresluje průběžně, takže Markdown tabulka nebo blok
  kódu může chvíli vypadat divně a srovná se, až dorazí zbytek řádku.
