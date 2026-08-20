#!/bin/sh
set -eu

curl --silent -f \
  http://phab.localhost:80/api/user.whoami \
  -d api.token=cli-aaaaaaaaaaaaaaaaaaaaaaaaaaaa | grep -q verified
