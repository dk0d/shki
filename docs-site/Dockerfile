# Build context: docs-site/ (set Build Path accordingly in Dokploy).
FROM oven/bun:1 AS build
WORKDIR /app

# Canonical site URL used for sitemaps and social cards; set as a build arg in
# the deploy environment (e.g. https://docs.example.com).
ARG DOCS_SITE
ENV DOCS_SITE=${DOCS_SITE}

COPY package.json bun.lock ./
RUN bun install --frozen-lockfile
COPY . .
RUN bun run build

FROM nginx:1.27-alpine
COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /app/dist /usr/share/nginx/html
EXPOSE 80
