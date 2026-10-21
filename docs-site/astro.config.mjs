// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import starlightThemeTerminalPlugin from "starlight-theme-terminal";
import starlightVersions from "starlight-versions";
import svelte from "@astrojs/svelte";
import tailwindcss from "@tailwindcss/vite";
import mermaid from "astro-mermaid";
import versionsFile from "./versions.json";
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
      expressiveCode: {
        themes: [
          {
            name: "shki-dark",
            type: "dark",
            colors: {
              "editor.background": "#1e231e",
              "editor.foreground": "#d4cec3",
            },
            tokenColors: [
              {
                scope: ["comment", "punctuation.definition.comment"],
                settings: { foreground: "#b5b0a6" },
              },
              { scope: ["keyword", "storage"], settings: { foreground: "#e5a072" } },
              { scope: ["string", "constant"], settings: { foreground: "#f2c5a4" } },
              { scope: ["entity", "support", "variable"], settings: { foreground: "#eee8de" } },
              { scope: "markup.inserted", settings: { foreground: "#b9c79b" } },
              { scope: ["markup.deleted", "invalid"], settings: { foreground: "#e59a89" } },
            ],
          },
          {
            name: "shki-light",
            type: "light",
            colors: {
              "editor.background": "#fffdf7",
              "editor.foreground": "#3e453d",
            },
            tokenColors: [
              {
                scope: ["comment", "punctuation.definition.comment"],
                settings: { foreground: "#596053" },
              },
              { scope: ["keyword", "storage"], settings: { foreground: "#9b4b27" } },
              { scope: ["string", "constant"], settings: { foreground: "#79371b" } },
              { scope: ["entity", "support", "variable"], settings: { foreground: "#252b25" } },
              { scope: "markup.inserted", settings: { foreground: "#4b6336" } },
              { scope: ["markup.deleted", "invalid"], settings: { foreground: "#9b3c2b" } },
            ],
          },
        ],
        styleOverrides: {
          codeBackground: "var(--shki-surface)",
          codeForeground: "var(--sl-color-text)",
          codeFontFamily: "var(--sl-font-mono)",
          borderColor: "var(--sl-color-hairline)",
          frames: {
            terminalBackground: "var(--shki-surface)",
            terminalForeground: "var(--sl-color-text)",
          },
        },
      },
      components: {
        // Renders the starlight-versions switcher next to the terminal
        // theme's toggle; both plugins warn that the slot is taken — that's
        // expected, this override is the manual composition they ask for.
        ThemeSelect: "./src/components/ThemeSelect.astro",
        // Version-aware banner: warns on superseded releases, notes /next/,
        // stays silent on the latest release (also served at the site root).
        Banner: "./src/components/Banner.astro",
        // Keeps the normal page title without starlight-versions' notice,
        // which incorrectly labels the latest archive as outdated.
        PageTitle: "./src/components/PageTitle.astro",
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
          ],
        },
        {
          label: "Commands",
          items: [
            { label: "config", slug: "commands/config" },
            { label: "init", slug: "commands/init" },
            { label: "shadow", slug: "commands/shadow" },
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
        // The working docs (tracking main) are re-archived as the "next"
        // version on every build (scripts/refresh-next.mjs), and nginx serves
        // the latest release's archive at the site root — so / is the latest
        // release and /next/ is main. versions.json lists the archived
        // releases (newest first) and records which one is latest; the release
        // flow maintains it (`task docs:archive-tag` for backfills).
        ...(versionsFile.versions.length > 0
          ? [
              starlightVersions({
                versions: [{ slug: "next", label: "Next" }, ...versionsFile.versions],
                current: { label: "Next" },
              }),
            ]
          : []),
      ],
    }),
  ],
});
