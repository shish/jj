#!/bin/sh
set -eu

status=$(mysqladmin status -h localhost -u root -pphabricator 2>/dev/null)
uptime=$(echo "$status" | grep -oP 'Uptime: \K\d+' || true)

if [ -z "$uptime" ]; then
  echo "MySQL is not responding"
  exit 1
fi

if [ "$uptime" -lt 3 ]; then
  echo "MySQL is starting up (uptime: ${uptime}s, need >= 3s)"
  exit 1
fi

echo "MySQL is healthy (uptime: ${uptime}s)"
exit 0
