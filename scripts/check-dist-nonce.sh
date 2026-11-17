#!/bin/sh
# Every <script> and modulepreload tag of the built shell must carry the CSP
# nonce placeholder: the server swaps it for a per-response nonce, and a tag
# without it is blocked by the page's Content-Security-Policy in production.
# The file is joined into one line first, so a tag split across lines is checked
# as a whole.
set -eu
index="${1:-crates/krausen-ui/dist/index.html}"
placeholder='nonce="__KRAUSEN_CSP_NONCE__"'
[ -f "$index" ] || { echo "check-dist-nonce: $index not found (run trunk build)" >&2; exit 1; }
tags=$(tr '\n' ' ' < "$index" | grep -o '<script[^>]*>\|<link[^>]*modulepreload[^>]*>' || true)
[ -n "$tags" ] || { echo "check-dist-nonce: no script tags found in $index" >&2; exit 1; }
bad=$(printf '%s\n' "$tags" | grep -v -- "$placeholder" || true)
if [ -n "$bad" ]; then
  echo "check-dist-nonce: tags without the nonce placeholder in $index:" >&2
  printf '%s\n' "$bad" >&2
  exit 1
fi
echo "check-dist-nonce: $(printf '%s\n' "$tags" | wc -l | tr -d ' ') tag(s) carry the nonce placeholder"
