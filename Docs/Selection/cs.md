# Označování

Označené soubory jsou vstupem každé operace. Pokud není nic označeno, pracuje se
se souborem pod kurzorem.

| Klávesa | Akce |
| --- | --- |
| `Insert` | označí / odznačí položku pod kurzorem |
| `Space` | označí / odznačí a posune se o řádek níže |
| `Num +` | označí všechny soubory odpovídající masce |
| `Num -` | odznačí všechny soubory odpovídající masce |
| `Num *` | obrátí označení |
| `Ctrl+Num +` | označí vše |
| `Ctrl+Num -` | odznačí vše |
| `Alt+Num +` | označí soubory se stejnou příponou, jakou má soubor pod kurzorem |
| `Alt+Num -` | odznačí soubory s touto příponou |

Označení přežívá opětovné načtení adresáře: položky, které už neexistují, se
odstraní, ostatní zůstanou označené. Označení je samostatné pro každý panel.

Stavový řádek vždy ukazuje `N z M souborů označeno` včetně velikosti
označených souborů, takže si můžete výběr před smazáním či kopírováním
zkontrolovat.
