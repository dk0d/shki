// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import starlightThemeTerminalPlugin from "starlight-theme-terminal";
import mermaid from "astro-mermaid";

// https://astro.build/config
export default defineConfig({
  site: "https://dk0d.github.io",
  base: "/shki",
  integrations: [
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
      customCss: ["./src/styles/custom.css"],
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
            { label: "Typed Queries", slug: "guides/queries" },
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
            { label: "queries", slug: "commands/queries" },
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
      ],
      plugins: [starlightThemeTerminalPlugin()],
    }),
  ],
});
