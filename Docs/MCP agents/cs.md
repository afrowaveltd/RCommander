# MCP agenti

RCommander mluví protokolem Model Context Protocol (MCP) oběma směry.

## Aplikace jako MCP server

MCP server spustíte samostatně příkazem `rcommander mcp` přes standardní
vstup a výstup (stdio). Externí agenti pak mohou ovládat správce souborů: vypisovat panely a adresáře, navigovat, označovat
soubory, kopírovat, přesouvat, mazat, vytvářet adresáře, přejmenovávat, číst
a zapisovat textové soubory, hledat, zjišťovat volné místo a číst tuto nápovědu.

**Aktuálně funkční přenos serveru:** pouze stdio (`rcommander mcp`).
Nápověda příkazového řádku sice zmiňuje režim HTTP, ale funkce `run_mcp`
zatím jeho argumenty nezpracovává, takže HTTP **server** není zapojen.
Připojení k HTTP serverům v roli **klienta** níže je odlišná funkce.

Rizikové nástroje (mazání, přesun, přepis, zápis souborů) mohou vyžadovat
potvrzení. Server v takovém případě odpoví chybou vysvětlující, že akci musí
schválit uživatel, místo aby ji tiše provedl.

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
