FROM mysql:8.0
ENV MYSQL_ROOT_PASSWORD="phabricator"
ENV MYSQL_DATABASE="phabricator"
ADD healthcheck.sh /scripts/healthcheck.sh
RUN chmod +x /scripts/healthcheck.sh
HEALTHCHECK --start-period=120s --interval=120s \
  CMD /scripts/healthcheck.sh
