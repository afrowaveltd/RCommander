# MCP agenti

RCommander mluví protokolem Model Context Protocol (MCP) oběma směry.

## Co je MCP, na jedné stránce

MCP (Model Context Protocol) je JSON-RPC protokol pro *aplikace, které nabízejí schopnosti jiným
programům*. Má tři druhy schopností a znalost toho, který je který, vysvětlí většinu toho, co
připojená aplikace umí:

| Druh | Co to je | Příklad |
| --- | --- | --- |
| **nástroje** (tools) | akce, každá s JSON schématem argumentů | `create_icon(prompt, size)` z kreslicí aplikace |
| **zdroje** (resources) | čitelné položky adresované URL-podobným id | dokument, obrázek, řádek databáze |
| **prompty** (prompts) | šablony, které aplikace publikuje | „shrn toto jednání" pro poštovní aplikaci |

Klient se připojí, zeptá se, co server nabízí (`tools/list`, `resources/list`, `prompts/list`), a pak
volá to, co člověk povolil. RCommander je **obojí**: `rcommander mcp` je server, kterým mohou
ovládat jiní agenti, a panel agenta je klient, který se připojuje k jiným serverům.

Co to ale **neznamená**: nástroj není oprávnění. Když aplikace nabídne `delete_everything`, neznamená
to, že je to bezpečné ani povolené; co model smí volat, je věc RCommanderu a jde to přes stejná
oprávnění a schvalovací dialog jako všechno ostatní.

## Aplikace jako MCP server

MCP server spustíte samostatně příkazem `rcommander mcp` přes standardní
vstup a výstup (stdio). Externí agenti pak mohou ovládat správce souborů: vypisovat panely a adresáře, navigovat, označovat
soubory, kopírovat, přesouvat, mazat, vytvářet adresáře, přejmenovávat, číst
a zapisovat textové soubory, hledat, zjišťovat volné místo a číst tuto nápovědu.

**Aktuálně funkční přenos serveru:** pouze stdio (`rcommander mcp`).
HTTP **server** zapojený není; požadavek na něj (`--http`, `--token`) skončí
výslovnou chybou, místo aby se tiše ignoroval. Připojení k HTTP serverům v roli
**klienta** níže je odlišná funkce.

### Schvalování rizikových nástrojů

Nástroje, které mohou zničit data (mazání, přesun, přepis, zápis souborů a
gitové nástroje `git_commit`/`git_checkout`/`git_push`), jsou chráněné branou.
Způsob schvalování se volí na příkazové řádce:

| Příkaz | Co se stane |
| --- | --- |
| `rcommander mcp` | před spuštěním takového nástroje se **zeptá běžícího okna RCommanderu** (dialog se třemi odpověďmi), a když žádné okno neběží, **zeptá se na terminálu**; bez obojího je každý rizikový nástroj odmítnut |
| `rcommander mcp --yes` | na nic se neptá a nástroj spustí. Oprávnění (policy) i audit se přesto uplatní |
| `rcommander mcp --no-ask` | nikdy se neptá a nikdy nepovolí — agent jen pro čtení |

Když okno běží, je to stejný dialog jako u agenta uvnitř aplikace: **povolit
nyní**, **povolit v této relaci** a červené **povolit vždy** — a dialog řekne, že
agent čeká, protože zablokovaný klient se snadno přehlédne. Odpověď letí zpět po
smyčkovém socketu, jehož adresa i token pro tento běh jsou v souboru přístupném
jen vlastníkovi v runtime adresáři, takže se ptát mohou jen vaše vlastní procesy.
Socket sám nic nerozhoduje: rozhoduje člověk a okno odpověď předá dál.

Spolehlivost odpovědi na terminálu stojí na dvou detailech. Za prvé jde otázka na
**terminál**, ne na standardní výstup: ten nese protokol JSON-RPC, takže otázka
zapsaná tam by ho rozbila — a čtení odpovědi ze standardního vstupu by spolklo
zprávu. Za druhé je čímkoli jiným než výslovným `y`/`yes`/`ano` **ne**, takže
omylem odeslaný prázdný řádek ani zavřený terminál nikdy neschválí smazání.
Výchozí je odmítnutí a chybějící terminál není souhlas.

Schválení je jen jedna z bran. Nejdřív se kontrolují oprávnění (v nastavení
`[agent] profile`, `require_approval`, granty) a každé rozhodnutí se zapíše do
auditu s hashovým řetězem popsaným v tématu *Konfigurace*.

## Aplikace jako MCP klient

Panel agenta se připojí k libovolnému MCP serveru — místnímu příkazu i HTTP
koncovému bodu — a umožní spouštět jeho nástroje přímo z RCommanderu. Servery se
konfigurují v `[agent] clients` v konfiguračním souboru:

```toml
[[agent.clients]]
name = "filesystem"
transport = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/home"]
```

## Katalog nástrojů: co jste povolili, si pamatujeme

Když se server připojí poprvé, RCommander si přečte, co publikuje, a **seznam si zapamatuje**: názvy
nástrojů, schémata argumentů a odhad, co který dělá. Od té chvíle panel agenta ten seznam ukazuje s
třípolohovým přepínačem u každého nástroje a odpovědi přežijí restart.

| Přepínač | Co znamená |
| --- | --- |
| **povolit** | model ho smí volat. Volání je pořád policy request, pořád se schvaluje podle pravidel a pořád se audituje |
| **zeptat se** | pokaždé se objeví schvalovací dialog. Takhle začíná **každý** nástroj |
| **zakázat** | nástroj se modelu vůbec nenabízí |

### Jak se odhaduje nebezpečnost

Z **názvu a argumentů**, nikdy z popisu — popis píše autor serveru a může tvrdit cokoli, kdežto
parametr `recursive` u nástroje `delete_directory` je fakt.

| Vypadá jako | Protože |
| --- | --- |
| **čte** | `get_*`, `list_*`, `read_*`, `search_*`, `describe_*` … |
| **mění data** | `create_*`, `update_*`, `write_*`, `move_*`, `commit_*` … nebo argumenty zmiňující `recursive`, `everything`, `wildcard` |
| **ničivé** | `delete_*`, `remove_*`, `drop_*`, `send_*`, `push_*`, `exec_*`, `run_*`, `pay_*` … |
| **neznámé** | nic rozpoznatelného — bereme je stejně opatrně jako ničivé, nikdy jako bezpečné |

**Ničivý (ani neznámý) nástroj nelze přepnout na `povolit` kliknutím.** Potřebuje stejné napsané
potvrzení jako trvalé předběžné schválení souborů a důvod se zapíše do auditu. Kde operační systém
nabízí elevaci, je autoritou privilegovaný helper; do té doby je napsané slovo podpis člověka a
všechno ostatní zůstává vázáno na běžná práva vašeho účtu.

**Nástroj, který změní povahu, ztratí odpověď.** Když upgrade udělá z `read_file` něco, co bere
`recursive`, odhad se změní, staré `povolit` se zahodí a nástroj se zase ptá. Přejmenovaný nástroj
zděděnou permission nemá a nový začíná na `zeptat se` — upgrade tedy nemůže potichu rozšířit, co model
smí.

## Bezpečnost

Server zpřístupňuje jen to, co protokol inzeruje; nic se nespustí, dokud není
nástroj zavolán. Každé volání je vidět ve stavovém řádku a politiku potvrzování
si určujete sami. Při připojování **klienta** k HTTP MCP serveru používejte důvěryhodný
ověřený koncový bod a chraňte přístupové údaje. Nezaměňujte to za dosud
nezapojený HTTP **server** RCommanderu.

## Panel agenta

*Síť → MCP agenti* otevře panel. Každá položka `[[agent.clients]]`
v `settings.toml` se stane tlačítkem; stiskem se aplikace připojí k danému
serveru, vypíše jeho nástroje a umožní jeden vybrat, upravit jeho JSON argumenty
a spustit. Výsledek (nebo chyba vrácená serverem) se zobrazí pod volbou nástroje.

```toml
[[agent.clients]]
name = "filesystem"
transport = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/home"]
```

Přenos je `stdio` (spustí se příkaz a komunikuje se přes standardní vstup/výstup)
nebo `http` (koncový bod JSON-RPC, vyžaduje `url`). Položky, kterým chybí povinné
pole, se ignorují, takže překlep nemůže panel rozbít. Pomalé či nedostupné
servery hlásí časový limit místo zablokování okna.
