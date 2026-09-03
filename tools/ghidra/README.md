# Ghidra — static analysis of `OSOS_correct.bin`

Ghidra answers *who could call this*. The emulator answers *who did*. Both matter, and confusing
them has cost this project published conclusions more than once — a vtable slot has exactly one
static reference, of type `DATA`, and reads as "unreachable" right up until you notice what it is.

**Ghidra proposes candidates; `tools/ipod-boot/from-idle.sh` says which ones fire.**

## Files here

| | |
|---|---|
**The three shell scripts this table used to list — `serve.sh`, `bridge.sh`, `q.sh` — no longer
exist; they were rewritten as `ipod-boot` subcommands and the table was not updated.** A document
naming a file that is in no commit is the same defect class as a flag with no mechanism behind it,
so it is spelled out rather than quietly corrected.

| | |
|---|---|
| `ipod-boot ghidra serve [--status]` | open the project and check a program is actually loaded |
| `ipod-boot ghidra bridge` | the stdio MCP bridge — what Claude Code registers against |
| `ipod-boot ghidra q xref\|fn\|dec\|raw` | query from a shell, no MCP client needed |

**`--status` is the one worth running first.** It distinguishes *nothing listening* from *listening
with no program open*, and the second reads as success to everything else: `/list_functions`
answers, the port is up, and every query returns nothing at all.

## Setup

```sh
brew install ghidra                                  # 12.1.3 at time of writing
git clone https://github.com/bethington/ghidra-mcp    # Apache-2.0, into resources/vendor/
JAVA_HOME=/opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home \
GHIDRA_INSTALL_DIR=/opt/homebrew/Cellar/ghidra/<version>/libexec \
  ./gradlew buildExtension                            # -> build/distributions/GhidraMCP-7.0.0.zip
unzip -o build/distributions/GhidraMCP-7.0.0.zip \
  -d ~/Library/ghidra/ghidra_<version>_PUBLIC/Extensions/
```

**`mvn package` does not work and the line above used to say it did.** The pom resolves Ghidra's
own jars — `ghidra:DB`, `ghidra:Debugger-rmi-trace` and nine more — and they are not on Maven
Central, so it fails at dependency resolution before compiling anything. `gradlew buildExtension`
reads them out of `GHIDRA_INSTALL_DIR` instead. It also needs `JAVA_HOME` set explicitly: this
machine's `/usr/bin/java` is the stub, and `/usr/libexec/java_home` answers *"Unable to locate a
Java Runtime"*, so the wrapper dies before Gradle starts.

### A Ghidra upgrade silently uninstalls this, and the symptom names nothing

**This is the failure mode to recognise**, because it cost most of a session. Extensions live in the
**per-version user directory** — `~/Library/ghidra/ghidra_12.1.3_PUBLIC/Extensions/GhidraMCP/` — and
`extension.properties` carries `version=<the Ghidra release>`. `brew upgrade ghidra` makes a new
user directory with no extensions in it, and the old extension would be refused anyway because its
stamp names the old release.

What you see is `ipod-boot ghidra serve` printing **`Ghidra did not come up`** after two minutes,
which reads like a broken script or a busy machine. Ghidra *is* running; it simply has no plugin, so
nothing ever listens on 8089. `--status` says `nothing at http://127.0.0.1:8089`, which is the same
sentence you get when Ghidra was never started at all.

Both user directories survive the upgrade, so the diagnosis is one command:

```sh
ls ~/Library/ghidra/*/Extensions/          # the old release has GhidraMCP, the new one is empty
```

**Rebuild against the new release and install into the new user directory.** Do not put it in
`<install>/Extensions/Ghidra/` inside the Cellar — that works, and the next `brew upgrade` deletes
it again with no trace.

Then, from the repo root:

```sh
claude mcp add ghidra -- "$PWD/ipod-boot ghidra bridge"
```

The MCP tools appear after Claude Code restarts — servers are loaded at session start. Until then
`q.sh` reaches the same server directly.

**The third-party checkout is not in git.** It is 179 MB and 3 000+ files of code we did not write,
and the project's rule is *borrow freely to learn, never to depend*. `bridge.sh` defaults to
`resources/vendor/ghidra-mcp`; override with `GHIDRA_MCP_HOME`. That indirection is the whole
reason `bridge.sh` exists — registering Claude Code directly against a path inside gitignored
material means the integration breaks silently the next time that tree is rebuilt or cloned fresh,
and **an MCP server that fails to start is indistinguishable, from inside a session, from one that
has nothing to say.**

## Loading the image

The server analyses `resources/derived/fw/OSOS_correct.bin` — the 7 559 680-byte RetailOS image,
loaded flat at `0x10000000` and mirrored at `0`. Ghidra's addresses are the **unmirrored** ones
(`0x001acca8`, not `0x101acca8`); `--callers=` in the emulator wants the mirrored form. That mismatch
is a standing trap, noted in `NEXT.md`'s instrument table.

## Worked example — what this is good for

The question was why RetailOS never draws. `0x001acca8` is the widget's "show" dispatcher.

```sh
ipod-boot ghidra q xref 0x001acca8      # -> one caller: 0x001ae080, in FUN_001ae070
ipod-boot ghidra q xref 0x001ae070      # -> one reference, type DATA, from 0x0066db98
```

The class vtable is at `0x0066daf4`, so `0x0066db98` is slot `+0xa4` — `setVisible`. *Show* is
unreachable by any direct call in 7.5 MB; it exists only behind a virtual call. That is a fact no
amount of `--enterlog` produces, and it took two queries. See `research/10` Addendum 22.
