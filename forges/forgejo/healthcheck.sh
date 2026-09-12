#!/bin/sh
set -eu

curl --silent -f -o /dev/null http://localhost:3000/api/healthz
test -s /data/api-key
curl --silent -f -o /dev/null \
  -H "Authorization: token $(awk '{print $NF}' /data/api-key)" \
  http://localhost:3000/api/v1/user
