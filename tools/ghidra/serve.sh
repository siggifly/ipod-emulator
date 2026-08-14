#!/bin/sh
# Launch the GhidraMCP headless server — no GUI, no window, nothing to look at.
#
# `python -m tools.setup deploy` starts the *GUI* and hosts the endpoint inside it; killing the
# window kills the server. This is the standalone path from the project's own docker/entrypoint.sh.
set -eu
: "${GHIDRA_HOME:=/opt/homebrew/Cellar/ghidra/12.1.2/libexec}"
: "${MCP_JAR:=$HOME/dev/siggifly/personal/ideas/03-clickwheel-games/resources/reference/ghidra-mcp/target/GhidraMCP-7.0.0.jar}"
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
