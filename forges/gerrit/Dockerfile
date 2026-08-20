FROM gerritcodereview/gerrit:3.13.10 AS upstream
FROM scratch
COPY --from=upstream / /
ENV CANONICAL_WEB_URL=http://gerrit.localhost:8080/
ENV HTTPD_LISTEN_URL=
EXPOSE 29418 8080
ADD healthcheck.sh /healthcheck.sh
RUN chmod +x /healthcheck.sh
HEALTHCHECK --start-period=120s --interval=120s \
  CMD /healthcheck.sh
USER gerrit
ENTRYPOINT ["/entrypoint.sh"]
