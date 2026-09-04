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
RUN bun run build

# The root-built pages only exist as the archive source for the "next"
# version: prune them so nginx's fallback serves the latest release at /.
# Kept at the root: version dirs (next, X.Y.Z), assets, search index, 404,
# favicon, and sitemaps.
RUN cd dist && find . -maxdepth 1 -mindepth 1 \
      ! -name '_astro' ! -name 'pagefind' ! -name 'next' \
      ! -name '[0-9]*.[0-9]*.[0-9]*' ! -name '404.html' \
      ! -name 'favicon.png' ! -name 'sitemap*' ! -name 'robots.txt' \
      -exec rm -rf {} + \
    && LATEST="$(bun -e 'console.log(JSON.parse(await Bun.file("/app/versions.json").text()).latest)')" \
    && sed "s/__LATEST__/${LATEST}/g" /app/nginx.conf > /app/nginx.rendered.conf

FROM nginx:1.27-alpine
COPY --from=build /app/nginx.rendered.conf /etc/nginx/conf.d/default.conf
COPY --from=build /app/dist /usr/share/nginx/html
EXPOSE 80
