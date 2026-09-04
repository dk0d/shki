<script lang="ts">
  // Renders statically (no client directive needed) — shadcn-svelte demo
  // usage; see src/lib/components/ui for the available components.
  import { Badge } from "$lib/components/ui/badge";
  import * as Card from "$lib/components/ui/card";

  const dialects = [
    {
      name: "PostgreSQL",
      full: true,
      note: "declarative compile, diff, generate, codegen, typed queries",
    },
    {
      name: "MySQL",
      full: false,
      note: "migration runner, custom migrations, dump",
    },
    {
      name: "SQLite",
      full: false,
      note: "migration runner; declarative support planned",
    },
  ];
</script>

<Card.Root class="not-content my-6">
  <Card.Header>
    <Card.Title>Supported dialects</Card.Title>
  </Card.Header>
  <Card.Content class="flex flex-col gap-3">
    {#each dialects as dialect (dialect.name)}
      <div class="flex flex-wrap items-center gap-3">
        <Badge variant={dialect.full ? "default" : "secondary"}>
          {dialect.name}
        </Badge>
        <span class="text-sm text-muted-foreground">{dialect.note}</span>
      </div>
    {/each}
  </Card.Content>
</Card.Root>
