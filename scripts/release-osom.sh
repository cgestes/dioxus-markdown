#!/usr/bin/env bash
# Build ONLY Osom (macOS Osom.app + osom-web static bundle — no Margin or demos)
# and publish them to a GitHub Release via `gh`.
#
#   scripts/release-osom.sh osom-v0.1.0
set -euo pipefail

tag="${1:-}"
if [ -z "$tag" ]; then
  version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$(dirname "$0")/../crates/osom/Cargo.toml" | head -1)"
  echo "Usage: $0 <tag>   (e.g. osom-v$version)" >&2
  exit 2
fi

root="$(cd "$(dirname "$0")/.." && pwd)"
target="${CARGO_TARGET_DIR:-$root/target}"
dist="$target/osom-release/$tag"
rm -rf "$dist"
mkdir -p "$dist"

echo "==> Building Osom.app (macOS)..."
app="$("$root/scripts/bundle-osom.sh")"
arch="$(uname -m)"
macos_zip="$dist/Osom-$tag-macos-$arch.zip"
ditto -c -k --sequesterRsrc --keepParent "$app" "$macos_zip"

echo "==> Building osom-web (browser bundle for Tailscale hosting)..."
web_dir="$("$root/scripts/build-osom-web.sh")"
web_tar="$dist/osom-web-$tag.tar.gz"
tar -C "$web_dir" -czf "$web_tar" .

echo "==> Publishing GitHub Release $tag..."
gh release create "$tag" \
  "$macos_zip" \
  "$web_tar" \
  --title "Osom $tag" \
  --notes "$(cat <<NOTES
## Osom $tag

### macOS app
Download \`$(basename "$macos_zip")\`, unzip, and drag **Osom.app** to \`/Applications\`.

### Web app (self-hosted on your ZeroClaw machine via Tailscale)
Unpack \`$(basename "$web_tar")\` on the machine where ZeroClaw & SilverBullet run, then serve it over your tailnet:
\`\`\`sh
mkdir -p ~/osom-web && tar -xzf $(basename "$web_tar") -C ~/osom-web
tailscale serve --bg --https=8444 --set-path=/_space    http://127.0.0.1:3000
tailscale serve --bg --https=8444 --set-path=/_zeroclaw http://127.0.0.1:42617
tailscale serve --bg --https=8444 --set-path=/          ~/osom-web
\`\`\`
Then open \`https://<your-host>.<tailnet>.ts.net:8444\` and pair with \`zeroclaw gateway get-paircode --new\`.
NOTES
)"
