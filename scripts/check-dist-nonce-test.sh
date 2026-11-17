#!/bin/sh
# Self-test for check-dist-nonce.sh: a stamped shell passes, an unstamped or
# multi-line unstamped tag fails, a missing file fails.
set -u
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM
fail=0
expect() { # name want_status file
  sh "$here/check-dist-nonce.sh" "$3" >/dev/null 2>&1
  got=$?
  if [ "$got" -ne "$2" ]; then echo "FAIL $1 (exit $got, wanted $2)"; fail=1; else echo "PASS $1"; fi
}
P='nonce="__KRAUSEN_CSP_NONCE__"'
printf '<script %s>1</script>\n<script type="module" %s>2</script>\n<link rel="modulepreload" %s href="/a.js">\n' "$P" "$P" "$P" > "$tmp/ok.html"
expect "stamped shell passes" 0 "$tmp/ok.html"
printf '<script %s>1</script>\n<script type="module">2</script>\n' "$P" > "$tmp/bare.html"
expect "unstamped script fails" 1 "$tmp/bare.html"
printf '<script %s>1</script>\n<script\n  type="module"\n  src="/x.js"\n></script>\n' "$P" > "$tmp/multi.html"
expect "unstamped multi-line script fails" 1 "$tmp/multi.html"
printf '<script\n  type="module"\n  %s\n></script>\n' "$P" > "$tmp/multi-ok.html"
expect "stamped multi-line script passes" 0 "$tmp/multi-ok.html"
printf '<link rel="modulepreload" href="/a.js">\n<script %s>1</script>\n' "$P" > "$tmp/preload.html"
expect "unstamped modulepreload fails" 1 "$tmp/preload.html"
printf '<html></html>\n' > "$tmp/none.html"
expect "no scripts fails" 1 "$tmp/none.html"
expect "missing file fails" 1 "$tmp/absent.html"
[ "$fail" -eq 0 ] && echo "all checks passed"
exit "$fail"
