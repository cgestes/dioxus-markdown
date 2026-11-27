#!/usr/bin/env bash
# Build `osom-web` (the browser build of Osom, without Margin or demos) into
# target/osom-web/ so it can be served on the ZeroClaw host via Tailscale.
#
#   scripts/build-osom-web.sh
#   scripts/build-osom-web.sh --agent luc --space-web https://home.tailnet.ts.net:8443
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
target="${CARGO_TARGET_DIR:-$root/target}"
out="$target/osom-web"
agent=""
comment_agent="scribe"
space_web=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    --agent) agent="$2"; shift 2 ;;
    --comment-agent) comment_agent="$2"; shift 2 ;;
    --space-web) space_web="$2"; shift 2 ;;
    -h|--help)
      echo "Usage: $0 [--out DIR] [--agent ALIAS] [--comment-agent ALIAS] [--space-web URL]"
      exit 0
      ;;
    *) echo "Unknown option: $1" >&2; exit 2 ;;
  esac
done

bundle="$target/dx/osom-web/release/web/public"
rm -rf "$bundle" "$out"
(cd "$root" && dx bundle --web --release --package osom-web)
[ -f "$bundle/index.html" ] || { echo "No web bundle at $bundle" >&2; exit 1; }

mkdir -p "$out"
cp -R "$bundle/." "$out"

# Optional server-side defaults: the browser uses same-origin `/_space` and `/_zeroclaw`
# so `tailscale serve` avoids CORS completely.
if [ -n "$agent" ] || [ -n "$space_web" ]; then
  python3 - "$out/config.json" "${agent:-home}" "$comment_agent" "$space_web" <<'PY'
import json, sys
path, agent, comment_agent, space_web = sys.argv[1:5]
with open(path, "w", encoding="utf-8") as f:
    json.dump({
        "space": "/_space",
        "zeroclaw": "/_zeroclaw",
        "agent": agent,
        "comment_agent": comment_agent,
        "space_web": space_web,
    }, f, indent=2)
    f.write("\n")
PY
fi

echo "$out"
