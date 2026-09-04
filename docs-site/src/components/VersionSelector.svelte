<script lang="ts">
  // Header version switcher (replaces starlight-versions' VersionSelect.astro
  // UI). Options are computed server-side in ThemeSelect.astro; picking one
  // navigates to the same page in that version.
  import * as Select from "$lib/components/ui/select";

  interface VersionOption {
    label: string;
    url: string;
    current: boolean;
  }

  let { options, label }: { options: VersionOption[]; label: string } =
    $props();

  const selected = options.find((option) => option.current) ?? options[0];

  function navigate(url: string) {
    if (url && url !== selected.url) {
      window.location.pathname = url;
    }
  }
</script>

<Select.Root type="single" value={selected.url} onValueChange={navigate}>
  <Select.Trigger size="sm" aria-label={label} class="min-w-[6.5rem]">
    {selected.label}
  </Select.Trigger>
  <Select.Content>
    {#each options as option (option.url)}
      <Select.Item value={option.url} label={option.label} />
    {/each}
  </Select.Content>
</Select.Root>
