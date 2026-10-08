# Builder image: Node LTS plus a pinned TypeScript compiler.
# This is the only step allowed network access (package installation).
# Every app build runs in a container from this image with the network
# disabled, a read-only root filesystem and dropped capabilities.
FROM node:24-alpine
ENV NPM_CONFIG_UPDATE_NOTIFIER=false
RUN npm install --no-audit --no-fund -g typescript@5.6.3
WORKDIR /workspace
