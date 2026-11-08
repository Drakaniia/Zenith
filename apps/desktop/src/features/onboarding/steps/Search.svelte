<script lang="ts">
  import Choice from '../Choice.svelte';
  import Toggle from '$shared/ui/Toggle.svelte';
  import { onboarding } from '../onboarding.svelte';
  import { settings } from '$features/settings/settings.svelte';
  import { SEARCH_ENGINES } from '$shared/url';

  const s = $derived(settings.value);
  const engine = $derived(SEARCH_ENGINES[s.searchEngine] ?? SEARCH_ENGINES.google);
</script>

<fieldset>
  <legend class="sr">Search engine</legend>
  <div class="grid two">
    {#each Object.entries(SEARCH_ENGINES) as [id, e] (id)}
      <Choice name="fre-engine" value={id} selected={s.searchEngine} title={e.name} desc={e.blurb} onselect={(v) => onboarding.set('searchEngine', v)} />
    {/each}
  </div>
</fieldset>

<div class="rows">
  <div class="row">
    <div class="text">
      <b>Suggest searches as I type</b>
      <span>Sends what you type in the address bar to {engine.name}.</span>
    </div>
    <Toggle checked={s.searchSuggestions} label="Suggest searches as I type" onchange={(v) => onboarding.set('searchSuggestions', v)} />
  </div>
</div>
