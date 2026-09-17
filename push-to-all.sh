#!/bin/sh

jj git fetch --remote upstream
jj git push --remote origin --branch main
jj git push --remote forgejo --branch main
jj git push --remote gerrit --branch main
jj git push --remote phabricator --branch main
