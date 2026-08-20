FROM phabricator/phabricator:latest
ADD healthcheck.sh /scripts/healthcheck.sh
ADD init.sh /scripts/init.sh
ADD init.php /scripts/init.php
ADD mysql-ping.php /scripts/mysql-ping.php
RUN chmod +x /scripts/healthcheck.sh /scripts/init.sh /scripts/mysql-ping.php

WORKDIR /opt/phabricator
# basic settings
RUN bin/config set mysql.host phabricator-mysql
RUN bin/config set mysql.user root
RUN bin/config set mysql.pass phabricator
RUN bin/config set phabricator.base-uri http://phab.localhost:8081/
RUN bin/config set differential.allow-self-accept true
# enable http server for diffusion (git) repositories
RUN bin/config set diffusion.allow-http-auth true
RUN bin/config set environment.append-paths '["/usr/lib/git-core"]'

HEALTHCHECK --start-period=120s --interval=120s \
  CMD /scripts/healthcheck.sh
