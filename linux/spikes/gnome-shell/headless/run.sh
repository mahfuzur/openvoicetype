#!/usr/bin/env bash
# Runs the spike extension in a headless GNOME Shell 50 (Ubuntu 26.04 in Docker) and probes it over D-Bus, so changes
# can be checked without a Linux desktop or logging out. GNOME Shell wants a system bus and logind: the container gets a
# system dbus-daemon and python-dbusmock's logind, as GNOME's own tests do.
#
#   linux/spikes/gnome-shell/headless/run.sh      (the first run installs GNOME Shell into the image, a few minutes)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IMAGE=openvoicetype-gnome-headless
UUID=openvoicetype-spike@mahfuzur.github.io

if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
  docker build -t "$IMAGE" - <<'DOCKERFILE'
FROM ubuntu:26.04
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends gnome-shell gjs dbus \
      libglib2.0-bin python3-gi python3-dbusmock mesa-vulkan-drivers libgl1-mesa-dri gnome-text-editor at-spi2-core \
    && rm -rf /var/lib/apt/lists/* && useradd -m tester
DOCKERFILE
fi

docker run --rm -v "$HERE/..:/spike:ro" "$IMAGE" bash -c "
  mkdir -p /run/dbus && dbus-daemon --system --fork 2>/dev/null
  python3 -m dbusmock --system --template logind >/dev/null 2>&1 &
  sleep 1
  runtime=/run/user/\$(id -u tester) extensions=/home/tester/.local/share/gnome-shell/extensions
  mkdir -p \$extensions \$runtime && cp -r /spike/$UUID \$extensions/
  chown -R tester /home/tester \$runtime && chmod 700 \$runtime
  su tester -c 'dbus-run-session -- bash /spike/headless/session.sh' 2>&1 | grep -E '^(ok|FAIL|info)|openvoicetype|JS ERROR|Traceback|Error:'
  exit \${PIPESTATUS[0]}
"
