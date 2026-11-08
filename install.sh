#!/bin/sh
# Install or remove iSCSI Manager.
#   ./install.sh                 build and install for this user (~/.local)
#   ./install.sh --system        install for everyone (/usr/local, uses sudo)
#   ./install.sh --prefix DIR    install under DIR
#   ./install.sh --no-build      don't build, use target/release/iscsi-manager
#   ./install.sh --uninstall     remove what was installed (same --system/--prefix)
set -eu

cd "$(dirname "$0")"

PREFIX="$HOME/.local"
SUDO=""
BUILD=1
UNINSTALL=0

while [ $# -gt 0 ]; do
    case "$1" in
        --system) PREFIX="/usr/local"; SUDO="sudo" ;;
        --prefix) shift; PREFIX="$1" ;;
        --no-build) BUILD=0 ;;
        --uninstall) UNINSTALL=1 ;;
        -h|--help) sed -n '2,7p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
    esac
    shift
done

[ "$(id -u)" = "0" ] && SUDO=""

BIN="$PREFIX/bin/iscsi-manager"
DESKTOP="$PREFIX/share/applications/iscsi-manager.desktop"
ICON="$PREFIX/share/icons/hicolor/scalable/apps/iscsi-manager.svg"

refresh_caches() {
    $SUDO update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
    $SUDO gtk-update-icon-cache -q -t "$PREFIX/share/icons/hicolor" 2>/dev/null || true
}

if [ "$UNINSTALL" = 1 ]; then
    $SUDO rm -f "$BIN" "$DESKTOP" "$ICON"
    refresh_caches
    echo "iSCSI Manager removed from $PREFIX."
    echo "Your iSCSI connections and saved targets (managed by open-iscsi) were not changed."
    exit 0
fi

if [ "$BUILD" = 1 ]; then
    if ! command -v cargo >/dev/null 2>&1; then
        echo "cargo (Rust) isn't installed. See docs/INSTALL.md, step 2." >&2
        exit 1
    fi
    echo "Building (the first build takes a few minutes)…"
    cargo build --release
fi

if [ ! -x target/release/iscsi-manager ]; then
    echo "target/release/iscsi-manager is missing. Run without --no-build." >&2
    exit 1
fi

$SUDO install -Dm755 target/release/iscsi-manager "$BIN"
$SUDO install -Dm644 packaging/iscsi-manager.svg "$ICON"
tmp="$(mktemp)"
sed "s|^Exec=.*|Exec=$BIN|" packaging/iscsi-manager.desktop > "$tmp"
$SUDO install -Dm644 "$tmp" "$DESKTOP"
rm -f "$tmp"
refresh_caches

echo "Installed iSCSI Manager:"
echo "  program   $BIN"
echo "  launcher  $DESKTOP"
echo "  icon      $ICON"
if ! command -v iscsiadm >/dev/null 2>&1 && [ ! -x /usr/sbin/iscsiadm ]; then
    echo
    echo "Note: open-iscsi isn't installed yet. Install it with:"
    echo "  sudo apt install open-iscsi"
    echo "or use the Install open-iscsi button in the app."
fi
