#!/bin/sh
set -eu

#test -s /var/opt/gitlab/api-key
#TOKEN=$(awk '{print $NF}' /var/opt/gitlab/api-key)
curl --silent -f -o /dev/null -H "PRIVATE-TOKEN: secret" http://localhost:8083/api/v4/user
