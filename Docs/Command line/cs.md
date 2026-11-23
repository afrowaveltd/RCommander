# Příkazový řádek

Příkazový řádek pod panely přijímá tři druhy vstupu:

1. **interní příkazy** — `cm_Copy`, `cm_SrcShort`, … Jsou to stejné identifikátory,
   jaké používají menu, tlačítkové lišty, klávesové zkratky i MCP agenti, takže
   cokoli jde zkratkou, lze napsat i sem.
2. **změny adresáře** — `cd <cesta>`, `cd ..`, `cd /media/usb`.
3. **příkazy operačního systému** — vše ostatní se předá shellu platformy
   s pracovním adresářem aktivního panelu.

Stiskněte `Enter` pro spuštění, `Alt+F8` pro vyvolání předchozího řádku
a `Ctrl+Enter` pro vložení názvu souboru pod kurzorem.

## Proč je to důležité pro automatizaci

Protože menu, tlačítková lišta, klávesové zkratky i MCP server mluví stejnými
identifikátory, může stejnou operaci řídit skript, tlačítko i AI agent.
Identifikátory `cm_` najdete v referenci příkazů této nápovědy a v paletě
příkazů aplikace.
