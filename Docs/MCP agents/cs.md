# MCP agenti

RCommander mluví protokolem Model Context Protocol (MCP) oběma směry.

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
| `rcommander mcp` | před spuštěním takového nástroje se **zeptá na terminálu**; bez terminálu (build agent, kontejner) je každý rizikový nástroj odmítnut |
| `rcommander mcp --yes` | na nic se neptá a nástroj spustí. Oprávnění (policy) i audit se přesto uplatní |
| `rcommander mcp --no-ask` | nikdy se neptá a nikdy nepovolí — agent jen pro čtení |

Spolehlivost odpovědi stojí na dvou detailech. Za prvé jde otázka na
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

## Bezpečnost

Server zpřístupňuje jen to, co protokol inzeruje; nic se nespustí, dokud není
nástroj zavolán. Každé volání je vidět ve stavovém řádku a politiku potvrzování
si určujete sami. Při připojování **klienta** k HTTP MCP serveru používejte důvěryhodný
ověřený koncový bod a chraňte přístupové údaje. Nezaměňujte to za dosud
nezapojený HTTP **server** RCommanderu.

## Panel agenta

*Síť ▸ MCP agenti* otevře panel. Každá položka `[[agent.clients]]`
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
