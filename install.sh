#!/bin/sh
# Installs the latest ghpr release binary for the current platform.
#   curl -fsSL https://raw.githubusercontent.com/planted-sam/ghpr/main/install.sh | sh
set -eu

REPO="planted-sam/ghpr"

os=$(uname -s)
arch=$(uname -m)

case "$os" in
  Darwin)
    case "$arch" in
      arm64) target="aarch64-apple-darwin" ;;
      *)
        echo "error: no prebuilt binary for macOS $arch — build from source: cargo install --git https://github.com/$REPO" >&2
        exit 1
        ;;
    esac
    ;;
  Linux)
    case "$arch" in
      x86_64) target="x86_64-unknown-linux-musl" ;;
      aarch64 | arm64) target="aarch64-unknown-linux-musl" ;;
      *)
        echo "error: no prebuilt binary for Linux $arch — build from source: cargo install --git https://github.com/$REPO" >&2
        exit 1
        ;;
    esac
    ;;
  *)
    echo "error: unsupported OS: $os" >&2
    exit 1
    ;;
esac

tag=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | head -n1 | cut -d'"' -f4)
if [ -z "$tag" ]; then
  echo "error: could not determine latest release tag" >&2
  exit 1
fi

url="https://github.com/$REPO/releases/download/$tag/ghpr-$tag-$target.tar.gz"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "Downloading ghpr $tag ($target)..."
curl -fsSL "$url" | tar -xz -C "$tmp"

if [ -w /usr/local/bin ]; then
  dest=/usr/local/bin
else
  dest="$HOME/.local/bin"
  mkdir -p "$dest"
fi

install -m 755 "$tmp/ghpr" "$dest/ghpr"
echo "Installed ghpr $tag to $dest/ghpr"

case ":$PATH:" in
  *":$dest:"*) ;;
  *) echo "note: $dest is not on your PATH — add it to your shell profile" ;;
esac
