# f1nder

An interactive command finder and pentest methodology checklist. Search and
browse the command library, then fill, complete, add, or drop parameters before
the finished command is handed back. Commands invoking `.exe` files, PowerShell
(`powershell` / `pwsh`), `.ps1` scripts, or recognized PowerShell cmdlets go to
the clipboard. Other commands print to the terminal after the interface closes.
Mentioning an `.exe` as a file argument does not trigger clipboard output.
F1 shows every key (j/k scrolls the overlay). Ctrl+Y copies the selected command
and exits from Search or Browse, skipping the fill dialog. Inside the fill
dialog, Ctrl+Y copies the current filled command and exits. Ctrl+Y always uses
the clipboard; Enter uses the destination rules above. Commands are never executed.

The three tabs are Search, Browse, and Methodology.

## The fill dialog

Enter on a command opens it. Each detected variable becomes a row, pre-filled
from the best source available — last used for this command → the sticky store →
`/etc/hosts` → shell history → environment → local tunnel IP → the template's
own text. Bare switches (`--no-pass`, `-k`, `2>/dev/null`) get rows too, marked
`⚑`, so they can be reached and removed.

| Key | Does |
|---|---|
| `⏎` | accept this row and move on; the last one outputs and exits |
| `⇥` / `→` | accept the ghost-text completion |
| `^X` | drop the whole parameter from this copy (toggle); deletes an added row |
| `^A` | add a new argument at the focused row's position in the command |
| `^U` | clear the row, reverting it to the template's own text |
| `^D` | disable autofill for this dialog and restore the entire base template, with literal tokens |
| `^P` / `^N` | cycle the ranked suggestions |
| `1`–`9` | select a value from the argument's dropdown |
| `Alt+←` / `Alt+→` | page through dropdown values when there are more than nine |
| `^Space` | reopen the suggestions dropdown |
| `Esc` | close the dropdown to type a value; Esc again cancels the dialog |
| `^T` | switch `/etc/hosts` target |
| `^Y` | copy to clipboard and exit, leaving remaining rows at their defaults |

The dropdown opens automatically for each argument with multiple suggestions.
Picking a value closes it. Typing also closes it; press Esc first when you want
to type a number from 1–9 instead of selecting a suggestion.

Ctrl+D resets all rows, including your edits, added arguments, and dropped
parameters. You can then edit manually, press Ctrl+Y to copy the literal template,
or use Enter to send it through shell integration. Autofill resumes the next
time you open a command.

Dropping and adding change only the copy you are about to use. The stored JSON
is never touched, and a dialog you Enter straight through reproduces the stored
command byte for byte.

Completion candidates come from the row's own suggestions, then every remembered
value of the same kind, and for file rows from the filesystem — type a `/` and
paths complete as you go.

## Search modes

`⇥` opens a numbered picker — `1` ALL, `2` TITLE, `3` HEADING, `4` CMD,
`5` RECENT — the same shape as the `⌘F` file filter. (While you are typing, `⇥`
first accepts a pending ghost-text completion.) On Browse the picker drives the
folder filter and offers the four field modes.

RECENT matches the same fields as ALL but ranks what you have actually run
first; with an empty query it lists only your history, so `⇥ 5` from a blank
prompt answers "what was I doing". `^R` opens the Recents overlay, which
reopens an entry with the values you used.

## Engagement profiles

`^P` switches profile. A profile scopes the three files that remember things —
the sticky variable store, the usage log, and the `env.sh` export — so one
client's hosts, domains, and credentials never complete into another's commands.
The active profile is shown at the bottom right. `n` creates one, `d` deletes it
and its history.

The `default` profile uses the original unscoped paths, so existing data keeps
working and there is nothing to migrate. Named profiles live in
`JSONs/profiles/<name>/`, `$XDG_DATA_HOME/f1nder/profiles/<name>/`, and
`$XDG_CACHE_HOME/f1nder/profiles/<name>/`.

## Shell prompt integration

Windows executable and PowerShell commands go to the clipboard, including with
shell integration enabled. Other commands print to stdout; shell integration
places them on your prompt, unrun, cursor at the end. Add one line to `~/.zshrc`
(or `~/.bashrc`):

```sh
eval "$(/path/to/f1nder --shell-init zsh)"   # or: bash
```

That defines two entry points, because there are two ways you reach the TUI:

| You do | You get |
|---|---|
| run `f1nder` | the command is waiting on your **next** prompt |
| press `^G` at a prompt | the command replaces what you were typing |
| run the binary by path, or from a tmux binding | same as the first row, via the drop file below |

The snippet hardcodes the absolute path of the binary that generated it, so
`f1nder` does not need to be on `$PATH` and the wrapper function cannot recurse
into itself. Re-run `--shell-init` if you move the binary. `--shell-init` with no
argument reads `$SHELL`.

The mechanism is `--print`: the TUI is drawn through `/dev/tty` so that stdout
carries nothing but the selected command, which the shell then hands to its own
line editor (`print -z` in zsh, `READLINE_LINE`/`history -s` in bash). Nothing is
ever executed — bash has no `print -z`, so there the plain `f1nder` wrapper puts
the command one Up-arrow away instead.

Only the wrapper function passes `--print`, so a binary started any other way
(`./f1nder`, an absolute path, a tmux popup) has no stdout anyone is reading. In
that case it prints non-Windows commands **and**, when using Enter, writes them to
`$XDG_CACHE_HOME/f1nder/prompt-<shell pid>.cmd`; the `precmd` hook installed by
the same eval reads that file, deletes it, and preloads the prompt. Naming the
file after the shell means a hook only ever sees its own line — no cross-talk
between terminals — and files a dead shell never collected are swept after an
hour. Mode `0600`, and `F1NDER_NO_LOG=1` turns it off along with the rest.

Without a controlling terminal, `--print` warns and draws the interface on stdout.

## Bulk import

Mass-add commands from a file of cmd-maker template blocks (`--- TITLE ---` …
`--- COMMANDS ---`):

```
f1nder --import commands.md
```

Each block is routed to `JSONs/cmds/<SOURCE-FILE>-CMDs.json`, gets a fresh id,
and duplicates (same title + command in the target file) are skipped, so the
command is safe to re-run. Run `f1nder --help` for usage.

## Audit and local data

`f1nder --audit-vars [FILTER]` checks fill detection and byte-for-byte template
round trips, and reports how many fields are droppable and how many bare
switches were found. With a FILTER it dumps the per-command detections, tagging
each row `value`, `flag`, or `added`.

Successful selections append to `history.jsonl` and merge their useful values
into `env.sh` (see the profile paths above). All three local stores —
`vars.json`, `history.jsonl`, `env.sh` — are created mode `0600`, but they can
contain passwords and NTLM hashes. Set `F1NDER_NO_LOG=1` to disable the log and
the export. Source `env.sh` only when you trust its contents.
