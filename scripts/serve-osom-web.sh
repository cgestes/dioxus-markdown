#!/usr/bin/env bash
# Expose `osom-web` on the ZeroClaw host over Tailscale Serve (tailnet only,
# never Funnel), proxying SilverBullet and ZeroClaw on the same HTTPS origin
# so the browser hits zero CORS issues.
#
# Run this on the machine where ZeroClaw & SilverBullet run:
#   scripts/serve-osom-web.sh /path/to/osom-web
#
# Options:
#   OSOM_HTTPS_PORT=8444          Tailscale Serve HTTPS port (default: 8444)
#   SILVERBULLET_LOCAL=http://127.0.0.1:3000
#   ZEROCLAW_LOCAL=http://127.0.0.1:42617
set -euo pipefail

site="${1:-}"
if [ -z "$site" ] || [ ! -f "$site/index.html" ]; then
  echo "Usage: $0 /path/to/osom-web-dir" >&2
  echo "Build the directory first with scripts/build-osom-web.sh (or unpack osom-web-*.tar.gz)." >&2
  exit 2
fi
site="$(cd "$site" && pwd)"

port="${OSOM_HTTPS_PORT:-8444}"
sb="${SILVERBULLET_LOCAL:-http://127.0.0.1:3000}"
zc="${ZEROCLAW_LOCAL:-http://127.0.0.1:42617}"

tailscale serve --bg --https="$port" --set-path=/_space "$sb"
tailscale serve --bg --https="$port" --set-path=/_zeroclaw "$zc"
tailscale serve --bg --https="$port" --set-path=/ "$site"

echo "Osom web is now served over your tailnet on HTTPS port $port."
tailscale serve status
