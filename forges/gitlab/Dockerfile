FROM gitlab/gitlab-ce:19.2.7-ce.0
ENV GITLAB_OMNIBUS_CONFIG="external_url 'http://gitlab.localhost:8083/'"
#ENV GITLAB_ROOT_USERNAME="admin"
ENV GITLAB_ROOT_PASSWORD="secret???"
ENV GITLAB_POST_RECONFIGURE_SCRIPT="/scripts/init.sh"
ADD healthcheck.sh /scripts/healthcheck.sh
ADD init.sh /scripts/init.sh
RUN chmod +x /scripts/healthcheck.sh /scripts/init.sh
# startup is ~240s on a good day...
HEALTHCHECK --start-period=600s --interval=120s \
  CMD /scripts/healthcheck.sh
