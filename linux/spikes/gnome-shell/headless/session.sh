#!/usr/bin/env bash
# Inside the container, as the test user, inside dbus-run-session (see run.sh): starts GNOME Shell headless with the
# spike extension and runs probe.py against it.
set -uo pipefail
XDG_RUNTIME_DIR="/run/user/$(id -u)"
export XDG_RUNTIME_DIR XDG_SESSION_TYPE=wayland
gsettings set org.gnome.shell disable-user-extensions false
gsettings set org.gnome.shell enabled-extensions "['openvoicetype-spike@mahfuzur.github.io']"
gnome-shell --headless --virtual-monitor 1280x800 >/tmp/shell.log 2>&1 &
for _ in $(seq 1 60); do
  gdbus call --session --dest io.github.mahfuzur.OpenVoiceType.Shell --object-path /io/github/mahfuzur/OpenVoiceType/Shell \
    --method io.github.mahfuzur.OpenVoiceType.Shell.GetVersion >/dev/null 2>&1 && break
  sleep 0.5
done
python3 "$(dirname "$0")/probe.py"
status=$?
grep -E "openvoicetype|JS ERROR" /tmp/shell.log
exit "$status"
