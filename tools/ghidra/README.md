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
brew install ghidra                                  # 12.1.2 at time of writing
git clone https://github.com/bethington/ghidra-mcp    # Apache-2.0
cd ghidra-mcp && mvn package                          # builds target/GhidraMCP-7.0.0.jar
```

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
