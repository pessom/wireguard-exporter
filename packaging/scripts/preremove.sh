#!/bin/sh
# Stop and disable the service on removal, not on upgrade.
# deb: $1 = remove | upgrade | ...; rpm: $1 = 0 (erase) | 1 (upgrade).
case "$1" in
    remove | 0)
        if [ -d /run/systemd/system ]; then
            systemctl disable --now wireguard_exporter.service >/dev/null 2>&1 || :
        fi
        ;;
esac
