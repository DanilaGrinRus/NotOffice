#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
VERSION="1.0.0"
NAME="NotOffice-${VERSION}-linux-x86_64"
OUT="packages/linux"
rm -rf "$OUT/$NAME" "$OUT/$NAME.tar.gz"
cargo build --release
mkdir -p "$OUT/$NAME"
cp target/release/notoffice "$OUT/$NAME/notoffice"
chmod +x "$OUT/$NAME/notoffice"
cp packaging/linux/README_LINUX_RU.md "$OUT/$NAME/"
cp USER_GUIDE_RU.md "$OUT/$NAME/"
cp packaging/linux/notoffice.desktop "$OUT/$NAME/"
cp packaging/linux/notoffice.png "$OUT/$NAME/"
tar -czf "$OUT/$NAME.tar.gz" -C "$OUT" "$NAME"
sha256sum "$OUT/$NAME.tar.gz" > "$OUT/$NAME.tar.gz.sha256"
echo "Built $OUT/$NAME.tar.gz"
