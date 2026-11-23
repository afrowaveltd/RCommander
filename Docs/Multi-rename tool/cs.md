# Nástroj hromadného přejmenování

`Ctrl+M` přejmenuje mnoho souborů najednou. Dialog má tři části: vzor názvu,
volitelné hledání a nahrazení, a živý náhled výsledku.

## Zástupné symboly

| Symbol | Význam |
| --- | --- |
| `[N]` | původní název bez přípony |
| `[n]`, `[U]` | totéž malými / velkými písmeny |
| `[E]`, `[e]` | přípona, jak je / malými písmeny |
| `[C]` | čítač od nastavené počáteční hodnoty |
| `[C+3]`, `[C-3]` | čítač jiného souboru v dávce |
| `[C4]` | čítač doplněný na čtyři místa |
| `[P]` | název nadřazeného adresáře |
| `[T1]`…`[T9]` | úrovně adresářů směrem nahoru |
| `[Y] [M] [D] [h] [m] [s]` | části data a času |
| `[Q]` | neznámý symbol zůstane beze změny |

Přípona má vlastní pole vzoru, takže výstup ve stylu `[N] — [E]` je snadný.

## Hledání a nahrazení

Prosté textové hledání a nahrazení se provádí na výsledku vzoru, což je
nejrychlejší cesta k odstranění předpony nebo sjednocení oddělovačů. Pro složité
případy je k dispozici režim regulárního výrazu.

## Bezpečnost

Nic se nepřejmenuje, dokud nepotvrdíte. Náhled ukazuje u každého označeného
souboru původní název vedle nového, kolize jsou odmítnuty a soubor, který by
skončil se stejným názvem, se přeskočí.
