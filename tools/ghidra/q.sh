#!/usr/bin/env bash
#
# Query the running Ghidra server over its REST API, without an MCP client.
#
# The MCP bridge is the right interface from inside an AI session, but MCP servers are loaded when a
# session starts — so a freshly registered one is unreachable until the session restarts. This is the
# escape hatch, and it is also the fastest way to ask a one-off question from a shell.
#
#   ./q.sh xref 0x001acca8         who references this address (calls AND data — a vtable slot
#                                  shows up as DATA, which reads as "unreachable" if you skip it)
#   ./q.sh fn 0x001acab0           which function contains this address
#   ./q.sh dec 0x0021a4f4          decompile the function containing this address
#   ./q.sh raw get_metadata        any endpoint, verbatim
#
# What this tool cannot tell you: whether any of it ever RAN. Ghidra answers "who could call this";
# only the emulator answers "who did". Every decisive finding in this project has been an arrival
# counter, and several wrong ones came from reading a static fact as a dynamic one. Pair it with
# `tools/ipod-boot/from-idle.sh`, which makes the dynamic half a 3-second question.
set -eu

: "${GHIDRA_URL:=http://127.0.0.1:8089}"

fail() { echo "$*" >&2; exit 1; }

curl -s -m 5 "$GHIDRA_URL/check_connection" | grep -q "Connection OK" \
  || fail "no Ghidra server at $GHIDRA_URL — start it with tools/ghidra/serve.sh"

pretty() { python3 -c 'import sys,json;d=json.load(sys.stdin);print(json.dumps(d,indent=2))' 2>/dev/null || cat; }

case "${1:-}" in
  xref) [ $# -eq 2 ] || fail "usage: q.sh xref ADDR"
        curl -s -m 10 "$GHIDRA_URL/get_xrefs_to?address=$2" | pretty ;;
  fn)   [ $# -eq 2 ] || fail "usage: q.sh fn ADDR"
        curl -s -m 10 "$GHIDRA_URL/get_function_by_address?address=$2" | pretty ;;
  dec)  [ $# -eq 2 ] || fail "usage: q.sh dec ADDR"
        curl -s -m 30 "$GHIDRA_URL/decompile_function_by_address?address=$2" \
          | python3 -c 'import sys,json;print(json.load(sys.stdin).get("decompiled",""))' ;;
  raw)  shift; [ $# -ge 1 ] || fail "usage: q.sh raw ENDPOINT[?args]"
        curl -s -m 30 "$GHIDRA_URL/$1" | pretty ;;
  *)    sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//' ;;
esac
