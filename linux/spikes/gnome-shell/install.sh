#!/usr/bin/env bash
# Installs (or with --remove, removes) the P1 spike extension for the current user. GNOME on Wayland only loads a new
# extension after you log out and back in; then enable it and run spike.py:
#
#   bash linux/spikes/gnome-shell/install.sh
#   (log out, log in)
#   gnome-extensions enable openvoicetype-spike@mahfuzur.github.io
#   /usr/bin/python3 linux/spikes/gnome-shell/spike.py
set -euo pipefail

UUID=openvoicetype-spike@mahfuzur.github.io
SOURCE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$UUID"
TARGET="${XDG_DATA_HOME:-$HOME/.local/share}/gnome-shell/extensions/$UUID"

if [[ "${1:-}" == --remove ]]; then
  gnome-extensions disable "$UUID" 2>/dev/null || true
  rm -rf "$TARGET"
  echo "Removed. Log out and in to unload it completely."
  exit 0
fi

command -v gnome-shell >/dev/null || { echo "This needs GNOME Shell." >&2; exit 1; }
mkdir -p "$(dirname "$TARGET")"
rm -rf "$TARGET"
cp -r "$SOURCE" "$TARGET"
# Ubuntu allows user extensions, but make sure they aren't switched off.
gsettings set org.gnome.shell disable-user-extensions false
echo "Installed $TARGET"
if gnome-extensions enable "$UUID" 2>/dev/null && gnome-extensions info "$UUID" 2>/dev/null | grep -q "State: ACTIVE"; then
  echo "It's running already. Next: /usr/bin/python3 linux/spikes/gnome-shell/spike.py"
else
  echo "Next: log out and back in (GNOME only finds new extensions at login; until then it says it \"does not exist\"),"
  echo "then run:"
  echo "    gnome-extensions enable $UUID"
  echo "    /usr/bin/python3 linux/spikes/gnome-shell/spike.py"
fi
