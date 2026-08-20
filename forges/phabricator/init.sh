#!/bin/sh
set -eu

# verify connectivity
php /scripts/mysql-ping.php phabricator-mysql root phabricator

# install db
bin/storage upgrade --force

# create admin user with fixed API token and VCS HTTP password
php /scripts/init.php

# start backend daemons
runuser -u www-data bin/phd start
