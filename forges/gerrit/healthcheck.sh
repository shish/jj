#!/bin/sh
set -eu

curl --silent -f http://admin:secret@localhost:8080/a/accounts/me
