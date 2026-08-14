#!/usr/bin/env bash
#
# The stdio MCP bridge — this is the stable path Claude Code is registered against.
#
#   claude mcp add ghidra -- <repo>/tools/ghidra/bridge.sh
#
# Why this file exists rather than registering the upstream entry point directly: the bridge lives
# inside a 179 MB Apache-2.0 third-party checkout that is deliberately NOT in git (see README.md).
# Registering a path into gitignored material means the integration silently breaks the moment that
# tree is rebuilt, moved, or cloned fresh on another machine — and an MCP server that fails to start
# looks, from inside a session, exactly like a server with nothing to say.
#
# So the tracked indirection is the point. Override the checkout location with GHIDRA_MCP_HOME.
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)

: "${GHIDRA_MCP_HOME:=$ROOT/resources/reference/ghidra-mcp}"

if [ ! -d "$GHIDRA_MCP_HOME" ]; then
  echo "ghidra-mcp checkout not found at $GHIDRA_MCP_HOME" >&2
  echo "see tools/ghidra/README.md — set GHIDRA_MCP_HOME or re-clone it there" >&2
  exit 1
fi

exec uv run --project "$GHIDRA_MCP_HOME" bridge-mcp-ghidra "$@"
