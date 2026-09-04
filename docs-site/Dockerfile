# Build context: docs-site/ (set Build Path accordingly in Dokploy).
FROM oven/bun:1.4-alpine AS base

WORKDIR /app

# Canonical site URL used for sitemaps and social cards; set as a build arg in
# the deploy environment (e.g. https://docs.example.com).
ARG DOCS_SITE
ENV DOCS_SITE=${DOCS_SITE}

COPY package.json bun.lock ./

FROM base AS deps
RUN bun install --frozen-lockfile

FROM deps AS build
COPY . .
RUN bun run build

FROM base as runtime
COPY --from=deps /app/node_modules ./node_modules
COPY --from=build /app/dist ./dist
ENV HOST=0.0.0.0
ENV PORT=80
EXPOSE 80
CMD ["bun", "./dist/server/entry.mjs"]
