# Ghidra MCP — headless

`ghidra-mcp-headless.sh` runs [bethington/ghidra-mcp](https://github.com/bethington/ghidra-mcp)
v7.0.0 as a background daemon. **No GUI, no window.** 226 REST endpoints.

`python -m tools.setup deploy` starts the *GUI* and hosts the endpoint inside it — killing the
window kills the server. This script is the standalone path, taken from the project's own
`docker/entrypoint.sh`.

```sh
./tools/ipod-boot/ghidra-mcp-headless.sh &          # ~20 s to initialise
curl -s http://127.0.0.1:8089/health                 # NOT /mcp/health — that is the GUI plugin's
```

## Loading our images

The parameter is **`file`**, not `file_path`, and `set_image_base` wants **`address`**, not
`base_address`. Both wrong guesses return a bare `... required` error that does not name the field.

```sh
curl -X POST localhost:8089/load_program -H 'Content-Type: application/json' \
  -d '{"file":"'"$PWD"'/resources/derived/re/iram.bin","language":"ARM:LE:32:v4t","compiler_spec":"default"}'
curl -X POST localhost:8089/set_image_base -H 'Content-Type: application/json' \
  -d '{"address":"0x40000000"}'
curl -X POST localhost:8089/run_analysis -H 'Content-Type: application/json' -d '{}'
curl "localhost:8089/decompile_function?address=0x4000bc0c"
```

`ARM:LE:32:v4t` is Ghidra's **exact ARM7TDMI** language — not an ARMv7 approximation.

> **`load_program` reports success without switching the current program.** Loading RetailOS
> returned `{"success":true,"program":"OSOS_correct.bin"}` while `/health` still said
> `iram.bin` — and the `set_image_base` that followed silently rebased **iram.bin** instead,
> reporting a perfectly plausible `40000000 -> 00000000`. Check `/health` after every
> `load_program`, and treat a base change as damage until the program name is confirmed.
>
> It reports success for a **path that does not exist**, too. That success value carries no
> information at all; `/health` and `list_open_programs` are the only sources of truth.

### Switching between loaded programs — you cannot, headless

`list_open_programs` shows everything the project holds, with `is_current`, `image_base` and
`function_count`:

```sh
curl -s localhost:8089/list_open_programs | python3 -m json.tool
```

`open_program` exists but answers **`"Opening programs requires GUI mode (PluginTool not
available)"`** — and only after you guess the parameter shape, since a POST body of any key returns
`"Program path is required"` and only `?path=` as a **GET query** reaches the real error.

So headless, *the first program loaded into a fresh server becomes the current one and stays that
way*. To work on a different image: kill the server, start it again, and load that image first.

```sh
pkill -f GhidraMCPHeadlessServer          # the old one holds the port AND the current program
nohup ./tools/ipod-boot/ghidra-mcp-headless.sh > /tmp/ghidra.log 2>&1 &
```

A server already running is easy to miss: the launcher fails with `Address already in use` while
`/health` answers perfectly well from the **old** process, so everything looks fine and the wrong
program is current.

### Measured on RetailOS

`OSOS_correct.bin`, `ARM:LE:32:v4t`, image base `0x00000000` (which makes Ghidra addresses equal
trace PCs — the file's byte 0 is execution address 0, confirmed by decoding `0xaf4a4` to the exact
`bl 0xc1648` the tracer reported). Auto-analysis finds **27 062 functions** in a few minutes.

It does **not** create functions for entry points reached only through data tables — every RTXC
task entry (`DiskMgrTask` and the rest) returns `No function found`. Read those with the tracer's
`--disasm=ADDR:COUNT` instead, which has the further advantage of reading the *running* machine:
large parts of RetailOS are scatter-loaded, and the file holds zeros where the code will be.

Get the images out of the running machine with `--save-region` (see the trace tool); the bootloader
is scatter-loaded into IRAM from NOR, so the flash file on disk is not what executes.

| image | region | base |
|---|---|---|
| bootloader | `iram` | `0x40000000` |
| NOR | `flash` | `0x20000000` |
| RetailOS | — | `0x10000000` |

## Measured on our bootloader

471 functions found in 6.7 s, against 411 call targets from our own `--callers` BL scan — so it
recovers boundaries that a branch scan alone misses. The decompilation of `0x4000bc0c` matched a
careful hand-decode instruction for instruction, including the ATA status-bit meanings.

## Importing our own symbols

RetailOS ships no symbol table, but the loader recovers 140 names from the image itself
(`trace --symbols`, see `extract_symbols`). Pushing them into Ghidra makes every decompilation
downstream readable, and names propagate through the call graph.

```sh
# rename where Ghidra already has a function; fall back to a label where it does not
curl -X POST localhost:8089/rename_function -H 'Content-Type: application/json' \
  -d '{"function_address":"0x00284b0c","new_name":"rb_DiskMgrTask"}'
curl -X POST localhost:8089/create_label -H 'Content-Type: application/json' \
  -d '{"address":"0x00284b0c","name":"rb_DiskMgrTask"}'
```

Measured on RetailOS: **92 renamed, 48 labelled, 0 failed.** `rename_function` wants
`function_address` (not `address`), and fails with `No function found` for anything Ghidra did not
make a function — which is most tail-call targets.

> **`create_function` and `rename_function` conflate functions by NAME.** Creating functions at two
> different addresses while a label of the same name existed produced **byte-identical
> decompilation for both**, and a subsequent rename of the second address reported renaming it *from
> the name just given to the first*. Never reuse a name, even a throwaway probe name, and treat two
> addresses decompiling identically as a tooling artefact until proven otherwise.
>
> `create_function` also guesses the body: it produced a 28-byte function where execution
> demonstrably flows on for hundreds of bytes, and a 4-byte one at a tail-call target. Bodies it
> invents are not evidence about function boundaries.
