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

## Co zatím není

Procházení repozitáře jako panelu (větve, tagy a commity jako složky), kopírování
souborů z revize a klonování jsou další kroky plánu; engine, na kterém staví, je hotový
a otestovaný.
