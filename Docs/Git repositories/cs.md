# Git repozitáře

RCommander čte Git repozitáře, aniž by vyžadoval nainstalovaný `git`: stejný kód běží
na Windows, Linuxu i Androidu.

## Vaše konfigurace Gitu

*Konfigurace ▸ Konfigurace Gitu…* nabídne načtení konfigurace Gitu z tohoto počítače.
Nejprve se zeptá, protože ta konfigurace může obsahovat tajemství:

| Položka | K čemu se používá |
| --- | --- |
| identita (`user.name`, `user.email`) | autorství commitů a zobrazení, komu commit patří |
| `credential.helper` | jak se na tomto počítači ukládají přihlašovací údaje |
| uložené údaje (`~/.git-credentials`) | přihlášení k remotu bez opakovaného zadávání hesla |
| `url.<base>.insteadOf` | rozbalení krátkých aliasů typu `gh:owner/repo` |
| `core.sshCommand`, `init.defaultBranch` | příkaz SSH a výchozí název větve |

Dialog vypíše přesně to, co našel — hostitele a názvy nápověd, nikdy heslo — a nabídne
tři odpovědi: **Povolit**, **Teď ne** a zaškrtávací pole *Zapamatovat si rozhodnutí*.
Odpověď se ukládá do `settings.toml` jako `[git] read_system_config = "ask" |
"allowed" | "denied"`, takže odmítnutí se už neopakuje, dokud ho nezměníte (nebo
nepoužijete *Přestat používat* ve stejném dialogu).

Hesla načtená z `store` nápovědy zůstávají v paměti pro relaci. Nikdy se neukládají do
`settings.toml`, nezobrazují se v dialogu a neobjevují se v žádné hlášce ani v logu.

## Čtení revizí

Engine pod panely otevře repozitář (hledá nadřazený, stejně jako git) a nabízí větve,
tagy, remote větve a log commitů a také obsah kterékoli revize. Nic v něm do repozitáře
nezapisuje.

## Repozitář jako panel

*Konfigurace ▸ Otevřít jako Git repozitář* přepne aktivní panel na repozitář, který
obsahuje jeho adresář. Panel pak ukazuje pracovní strom **a čtyři virtuální složky**:

| Složka | Co obsahuje |
| --- | --- |
| `branches` | všechny místní větve; otevřením se zobrazí strom té revize |
| `tags` | všechny tagy, stejný princip |
| `commits` | posledních 500 commitů jako `<krátké id> <popis>` |
| `remotes` | remote větve |

Cesta v panelu ukazuje větev v hranatých závorkách (`/home/já/projekt  [main]`) a stavový
řádek hlásí, který repozitář se otevřel. Uvnitř revize se prochází jako v běžném
adresáři a `..` jde o úroveň výš.

## Barvy, které řeknou, kde jste

Řádky v Git panelu se barví podle stavu, ne podle typu souboru:

| Stav | Barva |
| --- | --- |
| připraveno k commitu (přidáno, přejmenováno, smazané v indexu) | zelená tématu (`archive`) |
| změněno na disku (modifikováno, smazáno, změna typu) | varovná barva |
| nesledované | barva odkazů (`symlink`) |
| ignorované | tlumená barva (`hidden`) |
| konflikt sloučení | varovná barva |

Odstíny jsou role tématu, takže vysokokontrastní nebo terminálové téma je změní spolu se
vším ostatním.

## Úprava, commit, push

Otevřete soubor pomocí `F4`, upravte ho a stiskněte *Použít*: soubor se uloží, a protože
leží v repozitáři, commander nabídne commit. Dialog ukáže soubor, upravitelnou zprávu
commitu (šablona `[git] commit_message`, výchozí `Edit {name}`) a zaškrtávací pole
*Po commitu odeslat (push)*. Dvě zaškrtávací pole to zafixují:

* *Odteď commitovat každý uložený soubor* → `[git] commit_on_save = "always"`, takže se
  z dialogu stane commit na jedno uložení bez klikání;
* *Už se neptat* → `"never"`, po němž se ukládáním nikdy nic necommitne.

Commit použije identitu z vaší konfigurace Gitu, pokud jste commanderu dovolili ji číst, a
push běží na vlákně na pozadí, takže pomalý remote nikdy nezamrzne okno — stavový řádek
hlásí *odesílám…* a pak *Odesláno*. Přihlašovací údaje pro HTTPS remotes pocházejí z
úložiště údajů nebo z `store` nápovědy, kterou jste schválili; nic se neukládá na disk.

## Co zatím není

Kopírování souborů z revize, klonování remotu a zbývající zapisovací operace (stage
libovolných souborů, pull, merge, přepínání větví z panelu) jsou další kroky; engine už
umí číst stav, staged, commit, push, přepínat a vytvářet větve a na špinavém stromě raději
odmítne, než by ztratil úpravu.
