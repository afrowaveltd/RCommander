# Jazyky a RTL

Rozhraní se překládá jazykovými balíčky; výchozí je angličtina a s aplikací se
dodává čeština.

## Jazykové balíčky

Balíček je jediný soubor TOML v adresáři `locales` (`locales/de.toml`):

```toml
[meta]
code = "de"
name = "German"
native_name = "Deutsch"
direction = "ltr"      # nepovinné, jinak se odvodí z kódu

[strings]
"dialog.copy.title" = "Kopieren"
```

Soubor vložte vedle aplikace (nebo do uživatelského konfiguračního adresáře),
restartujte a vyberte jej v *Konfigurace → Možnosti → Jazyk*. Chybějící klíče se
doplní z angličtiny, takže částečný balíček je použitelný okamžitě.

## Jazyky zprava doleva

Jazyky psané zprava doleva (arabština, hebrejština, perština, urdština, …) se
poznají podle kódu jazyka a podle pole `direction` v balíčku. Je-li zvolený
jazyk RTL:

- panely, seznamy souborů, menu i dialogy se zrcadlí vodorovně,
- pruh cesty teče zprava doleva,
- prohlížeč vykresluje text zarovnaný doprava a editor se řídí dokumentem.

Míchání skriptů je podporováno: prohlížeč se dívá na první výrazný znak textu,
takže hebrejský soubor v anglickém rozhraní se zobrazí správně.

## Překlad této nápovědy

Tato nápověda je prostý Markdown: `Docs/<Téma>/<jazyk>.md`. Zkopírujte `en.md`
do svého jazyka, přeložte, restartujte aplikaci a téma se objeví ve vašem jazyce.
Témata bez překladu se zobrazí anglicky.
