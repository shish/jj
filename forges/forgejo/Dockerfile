FROM codeberg.org/forgejo/forgejo:16
ENV FORGEJO__server__ROOT_URL="http://forgejo.localhost:8082/"
ENV FORGEJO__security__INSTALL_LOCK="true"
ADD healthcheck.sh /scripts/healthcheck.sh
ADD init.sh /scripts/init.sh
RUN chmod +x /scripts/healthcheck.sh /scripts/init.sh
HEALTHCHECK --start-period=120s --interval=120s \
  CMD /scripts/healthcheck.sh
