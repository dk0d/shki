// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import starlightThemeTerminalPlugin from "starlight-theme-terminal";

// https://astro.build/config
export default defineConfig({
	site: "https://dk0d.github.io",
	base: "/shki",
	integrations: [
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
						{ label: "Dump A Live Database", slug: "guides/dump" },
						{ label: "Code Generation", slug: "guides/codegen" },
						{ label: "Typed Queries", slug: "guides/queries" },
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
