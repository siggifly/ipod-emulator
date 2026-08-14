#!/bin/sh
# Launch the GhidraMCP headless server — no GUI, no window, nothing to look at.
#
# `python -m tools.setup deploy` starts the *GUI* and hosts the endpoint inside it; killing the
# window kills the server. This is the standalone path from the project's own docker/entrypoint.sh.
set -eu
# Both defaults are one machine's layout. Neither ships here: Ghidra is installed separately, and
# the GhidraMCP jar is built from its own repository — nothing under `resources/` is committed.
: "${GHIDRA_HOME:=/opt/homebrew/Cellar/ghidra/12.1.2/libexec}"
: "${MCP_JAR:=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)/resources/reference/ghidra-mcp/target/GhidraMCP-7.0.0.jar}"
[ -f "$MCP_JAR" ] || { echo "no GhidraMCP jar at $MCP_JAR — build it and set MCP_JAR" >&2; exit 1; }
[ -d "$GHIDRA_HOME" ] || { echo "no Ghidra at $GHIDRA_HOME — set GHIDRA_HOME" >&2; exit 1; }
: "${PORT:=8089}"
: "${BIND:=127.0.0.1}"
export JAVA_HOME=/opt/homebrew/opt/openjdk@21
PATH="$JAVA_HOME/bin:$PATH"; export PATH

CP="$MCP_JAR"
for j in "$GHIDRA_HOME"/Ghidra/Framework/*/lib/*.jar \
         "$GHIDRA_HOME"/Ghidra/Features/*/lib/*.jar \
         "$GHIDRA_HOME"/Ghidra/Processors/*/lib/*.jar; do
  [ -f "$j" ] && CP="$CP:$j"
done

exec java -Xmx6g \
  -Dghidra.home="$GHIDRA_HOME" \
  -Dapplication.name=GhidraMCP \
  -classpath "$CP" \
  com.xebyte.headless.GhidraMCPHeadlessServer --port "$PORT" --bind "$BIND"
