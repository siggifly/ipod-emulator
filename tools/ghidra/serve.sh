#!/usr/bin/env bash
#
# Bring Ghidra up WITH THE PROGRAM IN IT, which is the only state worth calling "up".
#
# What this replaces, and why: this script used to launch `GhidraMCPHeadlessServer`, which answers
# /check_connection cheerfully and cannot ever hold a program. Both routes that would load one --
# /import_file and /open_program -- return "requires GUI mode (PluginTool not available)". So the
# headless server is permanently empty, every MCP tool answers "No program loaded", and from inside
# an AI session that is indistinguishable from a broken integration. The README one directory up
# warns about exactly this failure at the level of the *bridge*; it was live one layer deeper.
#
# The plugin lives in CodeBrowser. So: start the GUI on the project, then ask it to open the program
# in a CodeBrowser, then VERIFY a program is actually loaded before claiming success. A launcher
# that cannot fail is not a launcher, it is a wish.
#
#   ./serve.sh            bring it up and verify
#   ./serve.sh --status   say what is up right now, change nothing
#
# Build the project first, once (~6 min):
#
#   analyzeHeadless <resources>/derived/ghidra retailos \
#     -import <resources>/derived/fw/OSOS_correct.bin \
#     -processor ARM:LE:32:v4t -loader BinaryLoader -loader-baseAddr 0x0
#
# Loaded flat at base 0 because that is where RetailOS executes -- the low alias, not the
# 0x10000000 view it is loaded through. Ghidra's addresses then match the emulator's PCs directly.
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)

: "${GHIDRA_URL:=http://127.0.0.1:8089}"
: "${JAVA_HOME:=/opt/homebrew/opt/openjdk@21}"
: "${PROJECT:=$ROOT/resources/derived/ghidra/retailos.gpr}"
: "${PROGRAM:=/OSOS_correct.bin}"

export JAVA_HOME
PATH="$JAVA_HOME/bin:$PATH"; export PATH

loaded() { curl -s -m 10 "$GHIDRA_URL/get_metadata" 2>/dev/null | grep -q '"program_name"'; }
up()     { curl -s -m 5 "$GHIDRA_URL/check_connection" 2>/dev/null | grep -q .; }

if [ "${1:-}" = "--status" ]; then
  if loaded; then
    curl -s -m 10 "$GHIDRA_URL/get_metadata" | python3 -m json.tool 2>/dev/null || true
  elif up; then
    echo "plugin is up but NO PROGRAM IS LOADED — this is the state that looks like success" >&2
    exit 1
  else
    echo "nothing at $GHIDRA_URL" >&2
    exit 1
  fi
  exit 0
fi

if loaded; then
  echo "already up with a program loaded"
  exit 0
fi

if ! up; then
  [ -f "$PROJECT" ] || { echo "no Ghidra project at $PROJECT — build it first, see the header" >&2; exit 1; }
  echo "starting Ghidra on $PROJECT …"
  nohup ghidraRun "$PROJECT" >/tmp/ghidra-gui.log 2>&1 &
  # The GUI takes the better part of a minute to get its class search and plugins up.
  for _ in $(seq 1 40); do up && break; sleep 3; done
  up || { echo "Ghidra did not come up; see /tmp/ghidra-gui.log" >&2; exit 1; }
fi

echo "opening $PROGRAM in a CodeBrowser …"
curl -s -m 120 -X POST "$GHIDRA_URL/tool/launch_codebrowser" \
  -H 'Content-Type: application/json' -d "{\"path\":\"$PROGRAM\"}" >/dev/null || true

for _ in $(seq 1 30); do loaded && break; sleep 3; done
loaded || {
  echo "CodeBrowser did not end up with a program loaded — do NOT trust query results" >&2
  exit 1
}
curl -s -m 10 "$GHIDRA_URL/get_metadata" | python3 -m json.tool 2>/dev/null || true
