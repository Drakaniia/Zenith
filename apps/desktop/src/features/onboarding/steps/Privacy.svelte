<script lang="ts">
  import Toggle from '$shared/ui/Toggle.svelte';
  import { onboarding } from '../onboarding.svelte';
  import { settings } from '$features/settings/settings.svelte';

  const s = $derived(settings.value);
</script>

<div class="rows">
  <div class="row">
    <div class="text">
      <b>Block ads and trackers</b>
      <span>Stops them loading in every tab, using EasyList and EasyPrivacy. Pages load faster too.</span>
    </div>
    <Toggle checked={s.blockerEnabled} label="Block ads and trackers" onchange={(v) => onboarding.set('blockerEnabled', v)} />
  </div>
  <div class="row">
    <div class="text">
      <b>Block pop-ups</b>
      <span>Windows a page tries to open without you clicking anything. You can still open each one.</span>
    </div>
    <Toggle checked={s.blockPopups} label="Block pop-ups" onchange={(v) => onboarding.set('blockPopups', v)} />
  </div>
  <div class="row">
    <div class="text">
      <b>Save browsing history</b>
      <span>Kept only on this PC. It powers address-bar suggestions and most visited sites.</span>
    </div>
    <Toggle checked={s.recordHistory} label="Save browsing history" onchange={(v) => onboarding.set('recordHistory', v)} />
  </div>
  {#if s.recordHistory}
    <div class="row sub">
      <label class="text" for="fre-history-days"><b>Keep history for</b></label>
      <select id="fre-history-days" class="field select" value={s.historyDays} onchange={(e) => onboarding.set('historyDays', Number(e.currentTarget.value))}>
        <option value={7}>1 week</option>
        <option value={30}>1 month</option>
        <option value={90}>3 months</option>
        <option value={365}>1 year</option>
        <option value={0}>Forever</option>
      </select>
    </div>
  {/if}
  <div class="row">
    <div class="text">
      <b>Install updates automatically</b>
      <span>Security fixes download in the background and install when you close Zenith.</span>
    </div>
    <Toggle checked={s.autoUpdate} label="Install updates automatically" onchange={(v) => onboarding.set('autoUpdate', v)} />
  </div>
</div>

<p class="note">Zenith sends no usage data. Nothing here leaves your PC.</p>

<style>
  /* Belongs to "Save browsing history" above it. */
  .rows .row.sub {
    padding-left: 34px;
  }
  .select {
    height: 32px;
    padding: 0 10px;
  }
</style>
