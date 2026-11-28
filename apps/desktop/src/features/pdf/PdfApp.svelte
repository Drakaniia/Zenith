<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import { PdfView, type ToolName } from './pdf-view';
  import { applyPageOp, placeStamp, type PageOp } from './pages';
  import SignaturePad, { type Signature } from './SignaturePad.svelte';
  import PagesPanel from './PagesPanel.svelte';

  const q = new URLSearchParams(location.search);
  const token = q.get('t') ?? '';
  const web = /^https?:/i.test(q.get('src') ?? '');
  const src = q.get('src') ?? '';
  const name = q.get('name') || 'document.pdf';
  /** Opened from disk, so Save writes the file in place. */
  const local = src.startsWith('file:');
  const safeDecode = (v: string) => {
    try {
      return decodeURIComponent(v);
    } catch {
      return v;
    }
  };
  /** The file name from the address, shown while the file downloads. */
  const fileLabel = q.get('name') || safeDecode(src.split(/[?#]/)[0].split('/').pop() || 'PDF');
  document.title = fileLabel;

  const TOOLS: [ToolName, string][] = [
    ['none', 'Select'],
    ['text', 'Text'],
    ['draw', 'Draw'],
    ['highlight', 'Highlight'],
  ];

  let container: HTMLDivElement;
  let view: PdfView | null = $state(null);
  let tick = $state(0);
  let error = $state('');
  /** Waiting for the browser to fetch the file (it reloads this page with a token). */
  let waiting = $state(!token && !q.has('failed'));
  let slow = $state(false);
  let busy = $state(false);
  let dirty = $state(false);
  let note = $state('');
  let panel = $state(false);
  let signing = $state(false);
  let stamp: Signature | null = $state(null);
  let color = $state('#e5484d');
  let password: { retry: boolean; value: string; done: (v: string | null) => void } | null = $state(null);
  /** Earlier bytes, for undoing page edits and signatures. */
  const history: Uint8Array[] = [];

  const info = $derived.by(() => {
    void tick;
    const v = view?.viewer;
    return {
      page: v?.currentPageNumber ?? 1,
      pages: v?.pagesCount ?? 0,
      zoom: Math.round((v?.currentScale ?? 1) * 100),
      tool: view?.tool ?? 'none',
      canUndo: !!view?.canUndo || history.length > 0,
    };
  });

  const ask = (retry: boolean) =>
    new Promise<string | null>((done) => (password = { retry, value: '', done }));

  function flash(text: string) {
    note = text;
    setTimeout(() => note === text && (note = ''), 2500);
  }

  async function open(bytes: Uint8Array) {
    await view!.open(bytes, ask, () => (dirty = true));
    view!.setColor(color);
  }

  onMount(() => {
    view = new PdfView(container, () => tick++);
    if (q.has('failed')) error = "Couldn't download this PDF.";
    if (!token) {
      const t = setTimeout(() => (slow = true), 15_000);
      return () => clearTimeout(t);
    }
    (async () => {
      const res = await fetch(`/doc/${token}`).catch(() => null);
      if (!res?.ok) {
        // The token died with the last session: load the original again, it comes back here.
        if (web) return location.replace(src);
        error = local
          ? `This PDF is no longer open. Open ${safeDecode(src.replace(/^file:\/+/, ''))} again.`
          : 'This PDF is no longer available.';
        return;
      }
      await open(new Uint8Array(await res.arrayBuffer()));
    })().catch((e) => (error = `Couldn't open this PDF: ${e?.message ?? e}`));

    const unsaved = (e: BeforeUnloadEvent) => dirty && e.preventDefault();
    addEventListener('beforeunload', unsaved);
    return () => removeEventListener('beforeunload', unsaved);
  });

  /** Runs an edit that rewrites the file, then shows the result. */
  async function edit(change: (bytes: Uint8Array) => Promise<Uint8Array>) {
    if (!view?.doc || busy) return;
    busy = true;
    try {
      const before = await view.bytes();
      const after = await change(before);
      if (after === before) return;
      history.push(before);
      await open(after);
      dirty = true;
    } catch (e) {
      flash(`Couldn't change the PDF: ${(e as Error)?.message ?? e}`);
    } finally {
      busy = false;
      tick++;
    }
  }

  const pageOp = (op: PageOp) => edit((b) => applyPageOp(b, op));

  async function undo() {
    if (view?.canUndo) return view.undo();
    const prev = history.pop();
    if (prev) await open(prev);
    tick++;
  }

  async function save() {
    if (!view?.doc || busy) return;
    if (!local) return saveAs();
    busy = true;
    try {
      const res = await fetch(`/save/${token}`, { method: 'POST', body: (await view.bytes()) as BodyInit });
      if (!res.ok) throw new Error(await res.text());
      dirty = false;
      flash('Saved');
    } catch (e) {
      flash(`Couldn't save: ${(e as Error)?.message || 'the file may be read-only or open elsewhere'}`);
    } finally {
      busy = false;
    }
  }

  /** The browser asks where to save (see host/downloads.rs). */
  async function saveAs() {
    if (!view?.doc) return;
    const url = URL.createObjectURL(new Blob([(await view.bytes()) as BlobPart], { type: 'application/pdf' }));
    const a = Object.assign(document.createElement('a'), { href: url, download: name });
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 60_000);
    if (!local) dirty = false;
  }

  function setTool(t: ToolName) {
    stamp = null;
    if (view) view.tool = t;
  }

  function pickColor() {
    view?.setColor(color);
  }

  function signed(sig: Signature | null) {
    signing = false;
    if (!sig) return;
    setTool('none');
    stamp = sig;
    flash('Click where the signature goes');
  }

  /** Places the pending signature where the page was clicked. */
  function place(e: MouseEvent) {
    if (!stamp || !view) return;
    const pageEl = (e.target as HTMLElement).closest<HTMLElement>('.page[data-page-number]');
    if (!pageEl) return;
    e.preventDefault();
    const index = Number(pageEl.dataset.pageNumber) - 1;
    const pv = view.viewer.getPageView(index);
    const r = pageEl.getBoundingClientRect();
    const [x, y] = pv.viewport.convertToPdfPoint(e.clientX - r.left, e.clientY - r.top);
    const width = 160;
    const s = stamp;
    stamp = null;
    void edit((b) => placeStamp(b, { png: s.png, page: index, x, y, width, height: (width * s.height) / s.width }));
  }

  function goTo(e: Event) {
    const n = Number((e.currentTarget as HTMLInputElement).value);
    if (view && n >= 1 && n <= info.pages) view.viewer.currentPageNumber = n;
  }

  function keys(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === 's') {
      e.preventDefault();
      void (e.shiftKey ? saveAs() : save());
    } else if (e.key === 'Escape' && stamp) {
      stamp = null;
    }
  }
</script>

<svelte:window onkeydown={keys} />

<div class="app">
  <header class="bar">
    <button class="icon" class:on={panel} title="Pages" onclick={() => (panel = !panel)}><Icon name="layers" /></button>
    <span class="pages">
      <input aria-label="Page" type="number" min="1" max={info.pages} value={info.page} onchange={goTo} />
      <span class="muted">/ {info.pages}</span>
    </span>
    <span class="sep"></span>
    <button class="icon" title="Zoom out" onclick={() => view?.zoom(1 / 1.2)}><Icon name="zoomOut" /></button>
    <button class="zoom" title="Fit to width" onclick={() => view && (view.viewer.currentScaleValue = 'page-width')}>{info.zoom}%</button>
    <button class="icon" title="Zoom in" onclick={() => view?.zoom(1.2)}><Icon name="zoomIn" /></button>
    <span class="sep"></span>
    <div class="tools" role="group" aria-label="Tools">
      {#each TOOLS as [t, label] (t)}
        <button class:on={info.tool === t && !stamp} aria-pressed={info.tool === t && !stamp} onclick={() => setTool(t)}>{label}</button>
      {/each}
      <button class:on={signing || !!stamp} aria-pressed={signing || !!stamp} onclick={() => (signing = true)}>Sign</button>
    </div>
    <input class="color" type="color" title="Colour" aria-label="Colour" bind:value={color} onchange={pickColor} />
    <button class="icon" title="Undo" disabled={!info.canUndo} onclick={undo}><Icon name="undo" /></button>
    <span class="grow"></span>
    {#if note}<span class="note" role="status">{note}</span>{/if}
    {#if local}
      <button class="btn primary" disabled={busy || !dirty} title="Save (Ctrl+S)" onclick={save}><Icon name="save" size={15} /> Save</button>
    {/if}
    <button class="btn" class:primary={!local} disabled={busy} title="Save a copy (Ctrl+Shift+S)" onclick={saveAs}>
      <Icon name="download" size={15} /> Save as
    </button>
  </header>

  <div class="body">
    {#if panel && view?.doc}
      <PagesPanel doc={view.doc} current={info.page} {busy} onop={pageOp} ongo={(n) => (view!.viewer.currentPageNumber = n)} />
    {/if}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="stage" class:placing={!!stamp} class:shifted={panel} bind:this={container} onclickcapture={place}>
      <div class="pdfViewer"></div>
    </div>
    {#if error}
      <div class="error">
        <Icon name="alert" size={26} /><p>{error}</p>
        {#if web}<button class="btn" onclick={() => location.replace(src)}>Try again</button>{/if}
      </div>
    {:else if waiting}
      <div class="error" role="status">
        <p>Opening {fileLabel}…</p>
        {#if slow && web}<button class="btn" onclick={() => location.replace(src)}>Try again</button>{/if}
      </div>
    {/if}
  </div>
</div>

{#if signing}
  <SignaturePad ondone={signed} />
{/if}

{#if password}
  <div class="scrim">
    <form
      class="dialog"
      onsubmit={(e) => {
        e.preventDefault();
        password!.done(password!.value);
        password = null;
      }}
    >
      <h2>{password.retry ? 'Wrong password' : 'This PDF is protected'}</h2>
      <!-- svelte-ignore a11y_autofocus -->
      <input type="password" placeholder="Password" autofocus bind:value={password.value} />
      <div class="row">
        <button type="button" class="btn" onclick={() => { password!.done(null); password = null; error = 'This PDF needs a password.'; }}>Cancel</button>
        <button class="btn primary">Open</button>
      </div>
    </form>
  </div>
{/if}

<style>
  :global(:root) {
    --bg: #f3f3f5;
    --bar: #ffffff;
    --text: #1c1c1f;
    --muted: #6b6b73;
    --line: #e2e2e6;
    --field: #ececef;
    --accent: #6e56cf;
    --on-accent: #fff;
    color-scheme: light dark;
  }
  @media (prefers-color-scheme: dark) {
    :global(:root) {
      --bg: #1b1b1e;
      --bar: #26262a;
      --text: #ececf0;
      --muted: #9a9aa3;
      --line: #36363c;
      --field: #333338;
    }
  }
  :global(html, body) {
    margin: 0;
    height: 100%;
    background: var(--bg);
    color: var(--text);
    font: 13px/1.4 system-ui, 'Segoe UI', sans-serif;
  }
  .app {
    height: 100vh;
    display: flex;
    flex-direction: column;
  }
  .bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 10px;
    background: var(--bar);
    border-bottom: 1px solid var(--line);
    overflow-x: auto;
  }
  button {
    font: inherit;
    color: inherit;
    border: 0;
    background: none;
    border-radius: 6px;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  button:not(:disabled):hover {
    background: var(--field);
  }
  button:focus-visible,
  input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .icon {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
  }
  .on,
  .on:hover {
    background: color-mix(in srgb, var(--accent) 18%, transparent) !important;
    color: var(--accent);
  }
  .pages {
    display: flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }
  .pages input {
    width: 44px;
    padding: 4px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--field);
    color: inherit;
    text-align: center;
  }
  .muted {
    color: var(--muted);
  }
  .sep {
    width: 1px;
    height: 20px;
    margin: 0 4px;
    background: var(--line);
  }
  .zoom {
    min-width: 52px;
    height: 30px;
  }
  .tools {
    display: flex;
    gap: 2px;
    padding: 2px;
    border-radius: 8px;
    background: var(--field);
  }
  .tools button {
    padding: 5px 10px;
  }
  .color {
    width: 30px;
    height: 30px;
    padding: 2px;
    border: 0;
    background: none;
    cursor: pointer;
  }
  .grow {
    flex: 1;
  }
  .note {
    color: var(--muted);
    white-space: nowrap;
    margin-right: 6px;
  }
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    white-space: nowrap;
    background: var(--field);
  }
  .btn.primary {
    background: var(--accent);
    color: var(--on-accent);
  }
  .btn.primary:not(:disabled):hover {
    background: color-mix(in srgb, var(--accent) 88%, black);
  }
  .body {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
  }
  /* pdf.js needs an absolutely positioned scroll container. */
  .stage {
    position: absolute;
    inset: 0;
    overflow: auto;
  }
  .stage.shifted {
    left: 200px;
  }
  .stage.placing :global(.page) {
    cursor: crosshair;
  }
  .error {
    position: absolute;
    inset: 0;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 8px;
    padding: 24px;
    text-align: center;
    color: var(--muted);
    background: var(--bg);
  }
  .scrim {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: rgb(0 0 0 / 0.35);
    z-index: 100;
  }
  .dialog {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: min(340px, calc(100vw - 32px));
    padding: 20px;
    border-radius: 12px;
    background: var(--bar);
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.3);
  }
  .dialog h2 {
    margin: 0;
    font-size: 16px;
  }
  .dialog input {
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--field);
    color: inherit;
    font: inherit;
  }
  .row {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
