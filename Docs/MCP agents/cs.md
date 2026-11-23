# MCP agenti

RCommander mluví protokolem Model Context Protocol (MCP) oběma směry.

## Aplikace jako MCP server

Zapněte *Server* v *Konfigurace ▸ Možnosti ▸ Agent*. Externí agenti pak mohou
ovládat správce souborů: vypisovat panely a adresáře, navigovat, označovat
soubory, kopírovat, přesouvat, mazat, vytvářet adresáře, přejmenovávat, číst
a zapisovat textové soubory, hledat, zjišťovat volné místo a číst tuto nápovědu.

K dispozici jsou dva přenosy:

- **stdio** — agent spustí `rcommander mcp` a komunikuje JSON-RPC po standardním
  vstupu a výstupu. To očekává většina MCP klientů.
- **HTTP** — místní koncový bod pro klienty, kteří nemohou spouštět procesy.
  Nastavte token, chcete-li vyžadovat ověření.

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
si určujete sami. HTTP přenos berte jako každou jinou místní službu: připojte ho
na localhost a použijte token.

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
