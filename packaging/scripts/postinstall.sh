#!/bin/sh
# The service is not enabled or started on install:
#   systemctl enable --now wireguard_exporter
# On upgrade a running service is restarted with the new binary.
if [ -d /run/systemd/system ]; then
    systemctl daemon-reload >/dev/null 2>&1 || :
    systemctl try-restart wireguard_exporter.service >/dev/null 2>&1 || :
fi
