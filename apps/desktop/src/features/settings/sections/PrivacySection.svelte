<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import Toggle from '$shared/ui/Toggle.svelte';
  import { settings, type Settings } from '../settings.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { blockerApi } from '$features/blocker/api';
  import { historyApi } from '$features/history/api';
  import { settingsApi } from '../api';
  import type { BlockerStats } from '$features/blocker/types';

  const s = $derived(settings.value);
  let stats = $state<BlockerStats | null>(null);
  let updating = $state(false);
  let customList = $state('');
  let clearKinds = $state({ history: true, cookies: false, cache: true, downloads: false, permissions: false });
  let clearRange = $state('hour');
  let clearing = $state(false);

  function set<K extends keyof Settings>(key: K, value: Settings[K]) {
    settings.update({ [key]: value } as Partial<Settings>);
  }

  async function refreshBlocker(update = false) {
    updating = update;
    try {
      stats = await blockerApi.blockerConfigure(settings.enabledLists(), update, s.blockerDisabledSites);
      if (update) settings.update({ filterListsUpdated: Date.now() });
    } catch (e) {
      ui.toast(`Could not update filter lists: ${e}`, { icon: 'alert' });
    } finally {
      updating = false;
    }
  }

  function addCustomList() {
    const url = customList.trim();
    if (!/^https?:\/\//.test(url)) return ui.toast('Enter a list URL starting with https://', { icon: 'alert' });
    const id = 'custom-' + Math.abs([...url].reduce((h, c) => (h * 31 + c.charCodeAt(0)) | 0, 7)).toString(36);
    if (s.filterLists.some((l) => l.url === url)) return;
    settings.value.filterLists.push({ id, name: new URL(url).hostname, url, enabled: true, custom: true });
    settings.save(false);
    customList = '';
    void refreshBlocker(true);
  }

  async function clearData() {
    clearing = true;
    const since =
      clearRange === 'all' ? null : Date.now() - ({ hour: 3600e3, day: 86400e3, week: 7 * 86400e3, month: 30 * 86400e3 }[clearRange] ?? 0);
    const kinds: string[] = [];
    if (clearKinds.cookies) kinds.push('siteData');
    if (clearKinds.cache) kinds.push('cache');
    if (clearKinds.downloads) kinds.push('downloads');
    if (clearKinds.permissions) kinds.push('permissions');
    try {
      await settingsApi.clearBrowsingData(kinds, since, clearKinds.history);
      ui.toast('Browsing data cleared', { icon: 'check' });
    } catch (e) {
      ui.toast(`Could not clear data: ${e}`, { icon: 'alert' });
    } finally {
      clearing = false;
    }
  }

  onMount(() => {
    void refreshBlocker(false);
  });
</script>

<!-- Privacy -->
<section id="settings-privacy">
  <h2>Privacy & blocking</h2>
  <div class="group">
    <div class="row">
      <div class="text">
        <b>Block ads and trackers</b>
        <span>
          {#if stats}{stats.rules.toLocaleString()} rules · {stats.totalBlocked.toLocaleString()} requests blocked this session{:else}Network-level blocking in every tab.{/if}
        </span>
      </div>
      <Toggle checked={s.blockerEnabled} onchange={(v) => set('blockerEnabled', v)} label="Ad blocking" />
    </div>
    <div class="sub">
      <div class="sub-head">
        <span>Filter lists</span>
        <button class="btn ghost sm" disabled={updating} onclick={() => refreshBlocker(true)}>
          <Icon name="reload" size={14} />{updating ? 'Updating…' : 'Update now'}
        </button>
      </div>
      <div class="list-row builtin">
        <Icon name="shieldCheck" size={15} />
        <span class="grow">Zenith built-in list</span>
        <span class="faint">always on</span>
      </div>
      {#each s.filterLists as list (list.id)}
        {@const st = stats?.lists.find((l) => l.id === list.id)}
        <div class="list-row">
          <Toggle
            checked={list.enabled}
            label={list.name}
            onchange={(v) => {
              list.enabled = v;
              settings.save(false);
              void refreshBlocker(false);
            }}
          />
          <span class="grow ellipsis" title={list.url}>{list.name}</span>
          <span class="faint">
            {#if st?.error}<span class="err">{st.error}</span>{:else if list.enabled && st}{st.rules.toLocaleString()} rules{/if}
          </span>
          {#if list.custom}
            <button
              class="icon-btn sm"
              title="Remove list"
              onclick={() => {
                settings.value.filterLists = s.filterLists.filter((l) => l.id !== list.id);
                settings.save(false);
                void refreshBlocker(false);
              }}><Icon name="trash" size={14} /></button
            >
          {/if}
        </div>
      {/each}
      <div class="add-row">
        <input class="field grow" placeholder="Add a list URL (hosts or Adblock Plus format)" bind:value={customList} onkeydown={(e) => e.key === 'Enter' && addCustomList()} />
        <button class="btn" onclick={addCustomList}>Add</button>
      </div>
    </div>
    {#if s.blockerDisabledSites.length}
      <div class="sub">
        <div class="sub-head"><span>Blocking turned off on</span></div>
        {#each s.blockerDisabledSites as site (site)}
          <div class="list-row">
            <Icon name="shieldOff" size={15} />
            <span class="grow">{site}</span>
            <button
              class="icon-btn sm"
              title="Turn blocking back on"
              onclick={() => {
                settings.value.blockerDisabledSites = s.blockerDisabledSites.filter((x) => x !== site);
                settings.save(false);
                void blockerApi.blockerSetDisabledSites(settings.value.blockerDisabledSites);
              }}><Icon name="close" size={14} /></button
            >
          </div>
        {/each}
      </div>
    {/if}
    <div class="row">
      <div class="text"><b>Block pop-ups</b><span>Windows a page opens without you clicking anything.</span></div>
      <Toggle checked={s.blockPopups} onchange={(v) => set('blockPopups', v)} label="Block pop-ups" />
    </div>
    <div class="row">
      <div class="text"><b>Save browsing history</b></div>
      <Toggle checked={s.recordHistory} onchange={(v) => set('recordHistory', v)} label="Save history" />
    </div>
    <div class="row">
      <div class="text"><b>Keep history for</b></div>
      <select
        class="field select"
        value={s.historyDays}
        onchange={(e) => {
          const days = Number((e.currentTarget as HTMLSelectElement).value);
          set('historyDays', days);
          if (days > 0) void historyApi.historyPrune(days);
        }}
      >
        <option value={7}>1 week</option>
        <option value={30}>1 month</option>
        <option value={90}>3 months</option>
        <option value={365}>1 year</option>
        <option value={0}>Forever</option>
      </select>
    </div>
  </div>

  <h3>Clear browsing data</h3>
  <div class="group" data-search-whole>
    <div class="checks">
      <label><input type="checkbox" bind:checked={clearKinds.history} /> Browsing history</label>
      <label><input type="checkbox" bind:checked={clearKinds.cookies} /> Cookies and site data (signs you out)</label>
      <label><input type="checkbox" bind:checked={clearKinds.cache} /> Cached images and files</label>
      <label><input type="checkbox" bind:checked={clearKinds.downloads} /> Download history</label>
      <label><input type="checkbox" bind:checked={clearKinds.permissions} /> Site permissions</label>
    </div>
    <div class="row">
      <select class="field select" bind:value={clearRange}>
        <option value="hour">Last hour</option>
        <option value="day">Last 24 hours</option>
        <option value="week">Last 7 days</option>
        <option value="month">Last 4 weeks</option>
        <option value="all">All time</option>
      </select>
      <button class="btn danger" disabled={clearing} onclick={clearData}><Icon name="eraser" size={15} />{clearing ? 'Clearing…' : 'Clear data'}</button>
    </div>
  </div>
</section>

<style>
  section {
    padding-top: 8px;
    margin-bottom: 28px;
  }
  h2 {
    margin: 0 0 12px;
    font: 600 18px/1.3 var(--font-display);
  }
  h3 {
    margin: 22px 0 10px;
    font: 600 14px/1.3 var(--font-display);
    color: var(--text-2);
  }
  .group {
    border-radius: var(--radius-lg);
    background: var(--field);
    border: 1px solid var(--line);
    padding: 4px 16px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    min-height: 58px;
    padding: 10px 0;
    border-bottom: 1px solid var(--line);
  }
  .row:last-child {
    border-bottom: 0;
  }
  /* Controls keep their natural width; the description column gives way instead. */
  .row > :not(.text) {
    flex: none;
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .text b {
    font-weight: 500;
  }
  .text span {
    font-size: 12px;
    color: var(--text-3);
    line-height: 1.4;
  }
  .sub {
    padding: 10px 0 12px;
    border-bottom: 1px solid var(--line);
  }
  .sub-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-2);
    margin-bottom: 6px;
  }
  .btn.sm {
    height: 26px;
    padding: 0 8px;
    font-size: 12px;
  }
  .list-row {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 38px;
  }
  .list-row.builtin {
    color: var(--ok);
  }
  .list-row.builtin .grow {
    color: var(--text);
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .err {
    color: var(--danger);
  }
  .add-row {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }
  .select {
    height: 32px;
    padding: 0 8px;
  }
  .checks {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 0 6px;
  }
  .checks label {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  input[type='checkbox'] {
    accent-color: var(--accent);
    width: 15px;
    height: 15px;
  }
</style>
