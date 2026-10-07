#!/bin/sh
# Fix volume ownership for the runtime user, then drop privileges.
set -e
chown -R minio:minio /data
exec su-exec minio:minio /usr/bin/minio "$@"
