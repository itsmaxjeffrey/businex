FROM node:22-bookworm-slim AS build
RUN apt-get update && apt-get install -y --no-install-recommends python3 make g++ && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY package.json package-lock.json ./
COPY apps/server/package.json apps/server/package.json
COPY apps/web/package.json apps/web/package.json
COPY packages/shared/package.json packages/shared/package.json
RUN npm ci
COPY apps apps
COPY packages packages
RUN npm run build && npm test && npm prune --omit=dev

FROM node:22-bookworm-slim
ENV NODE_ENV=production BUSINEX_HOST=0.0.0.0 BUSINEX_PORT=8788 BUSINEX_DATA_DIR=/data BUSINEX_STATIC_DIR=/app/apps/web/dist BUSINEX_TERMINAL_ENABLED=false
WORKDIR /app
COPY --from=build --chown=node:node /app /app
RUN mkdir /data && chown node:node /data
USER node
EXPOSE 8788
VOLUME /data
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s CMD node -e "fetch('http://127.0.0.1:8788/api/health').then(r=>process.exit(r.ok?0:1)).catch(()=>process.exit(1))"
CMD ["node", "--import", "tsx", "apps/server/src/index.ts"]
