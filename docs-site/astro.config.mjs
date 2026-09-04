// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import starlightThemeTerminalPlugin from "starlight-theme-terminal";
import starlightVersions from "starlight-versions";
import svelte from "@astrojs/svelte";
import tailwindcss from "@tailwindcss/vite";
import mermaid from "astro-mermaid";
import versions from "./versions.json";
import remarkPinReleaseUrls from "./scripts/remark-pin-release-urls.mjs";

// https://astro.build/config
export default defineConfig({
  markdown: {
    remarkPlugins: [remarkPinReleaseUrls],
  },
  vite: {
    plugins: [tailwindcss()],
  },
  // Canonical site URL; set DOCS_SITE in the deploy environment.
  site: process.env.DOCS_SITE || "https://dk0d.github.io",
  integrations: [
    svelte(),
    mermaid({ theme: "dark", autoTheme: true }),
    starlight({
      title: "shki",
      favicon: "/favicon.png",
      description:
        "SQL-first declarative schema management and migration tooling for PostgreSQL, MySQL, and SQLite.",
      social: [
        {
          icon: "github",
          label: "GitHub",
          href: "https://github.com/dk0d/shki",
        },
      ],
      customCss: ["./src/styles/tailwind.css", "./src/styles/custom.css"],
      components: {
        // Renders the starlight-versions switcher next to the terminal
        // theme's toggle; both plugins warn that the slot is taken — that's
        // expected, this override is the manual composition they ask for.
        ThemeSelect: "./src/components/ThemeSelect.astro",
      },
      sidebar: [
        {
          label: "Getting Started",
          items: [
            { label: "Installation", slug: "getting-started/installation" },
            { label: "Quick Start", slug: "getting-started/quick-start" },
            { label: "How It Works", slug: "getting-started/how-it-works" },
          ],
        },
        {
          label: "Guides",
          items: [
            { label: "Declarative Schema", slug: "guides/declarative-schema" },
            { label: "Migrations", slug: "guides/migrations" },
            { label: "Code Generation", slug: "guides/codegen" },
            {
              label: "Typed Queries",
              slug: "guides/queries",
              badge: { text: "Alpha", variant: "caution" },
            },
          ],
        },
        {
          label: "Commands",
          items: [
            { label: "config", slug: "commands/config" },
            { label: "init", slug: "commands/init" },
            { label: "diff", slug: "commands/diff" },
            { label: "generate", slug: "commands/generate" },
            { label: "create", slug: "commands/create" },
            { label: "migrate", slug: "commands/migrate" },
            { label: "status", slug: "commands/status" },
            { label: "down", slug: "commands/down" },
            { label: "drop", slug: "commands/drop" },
            { label: "dump", slug: "commands/dump" },
            { label: "bootstrap", slug: "commands/bootstrap" },
            { label: "adopt", slug: "commands/adopt" },
            { label: "codegen", slug: "commands/codegen" },
            {
              label: "queries",
              slug: "commands/queries",
              badge: { text: "Alpha", variant: "caution" },
            },
          ],
        },
        {
          label: "Reference",
          items: [
            { label: "CLI", slug: "reference/cli" },
            { label: "Configuration", slug: "reference/configuration" },
            { label: "Concepts And Scope", slug: "reference/concepts" },
            { label: "Troubleshooting", slug: "reference/troubleshooting" },
          ],
        },
        { label: "Contributing", slug: "contributing" },
        { label: "Inspired By", slug: "inspired-by" },
      ],
      plugins: [
        starlightThemeTerminalPlugin(),
        // Versions live in versions.json (newest first); `task docs:version`
        // adds one and archives the current docs. An empty list disables
        // versioning until the first release is archived.
        ...(versions.length > 0 ? [starlightVersions({ versions })] : []),
      ],
    }),
  ],
});
