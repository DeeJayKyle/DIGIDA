#!/bin/bash
# Installs REA (https://github.com/morluto/rea) for Claude Code cloud sessions:
# registers its MCP server and the reverse-engineer-anything skill.
set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

NODE_VERSION=v24.22.1
REA_VERSION=6.8.0
NODE_DIR=/opt/node24

# REA needs Node >= 22.23.4; the container's default Node is older.
if [ ! -x "$NODE_DIR/bin/node" ]; then
  tmp=$(mktemp -d)
  curl -sSfL "https://nodejs.org/dist/$NODE_VERSION/node-$NODE_VERSION-linux-x64.tar.xz" | tar xJ -C "$tmp"
  mv "$tmp/node-$NODE_VERSION-linux-x64" "$NODE_DIR"
  rmdir "$tmp"
fi
export PATH="$NODE_DIR/bin:$PATH"

if [ "$(rea --version 2>/dev/null || true)" != "$REA_VERSION" ]; then
  npm install -g "rea-agents@$REA_VERSION" >&2
fi

cd /tmp && rea setup --client claude_code --yes >/dev/null
