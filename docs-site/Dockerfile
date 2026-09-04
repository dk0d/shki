# Build context: docs-site/ (set Build Path accordingly in Dokploy).
FROM oven/bun:1 AS base
WORKDIR /app

# Canonical site URL used for sitemaps and social cards; set as a build arg in
# the deploy environment (e.g. https://docs.example.com).
ARG DOCS_SITE
ENV DOCS_SITE=${DOCS_SITE}

FROM base AS deps
COPY package.json bun.lock ./
RUN bun install --frozen-lockfile

FROM deps AS build
COPY . .
# Builds the site and materializes the latest release's archive at the dist
# root (scripts/postbuild.mjs), so the image is plain static files.
RUN bun run build

FROM nginx:1.27-alpine
RUN rm -rf /usr/share/nginx/html/*
COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /app/dist /usr/share/nginx/html
EXPOSE 80
