<script lang="ts">
  import type { PDFDocumentProxy } from 'pdfjs-dist';
  import Icon from '$shared/ui/Icon.svelte';
  import type { PageOp } from './pages';

  let {
    doc,
    current,
    busy,
    onop,
    ongo,
  }: {
    doc: PDFDocumentProxy;
    current: number;
    busy: boolean;
    onop: (op: PageOp) => void;
    ongo: (page: number) => void;
  } = $props();

  const indices = $derived(Array.from({ length: doc.numPages }, (_, i) => i));

  /** Renders a page's thumbnail once it scrolls into view. */
  function thumb(canvas: HTMLCanvasElement, index: number) {
    const source = doc;
    const io = new IntersectionObserver(async ([entry]) => {
      if (!entry.isIntersecting) return;
      io.disconnect();
      const page = await source.getPage(index + 1);
      const base = page.getViewport({ scale: 1 });
      const viewport = page.getViewport({ scale: 150 / base.width });
      canvas.width = Math.ceil(viewport.width);
      canvas.height = Math.ceil(viewport.height);
      await page.render({ canvas, viewport }).promise.catch(() => {});
    });
    io.observe(canvas);
    return { destroy: () => io.disconnect() };
  }

  async function merge(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (file) onop({ kind: 'merge', bytes: new Uint8Array(await file.arrayBuffer()) });
  }
</script>

<aside aria-label="Pages">
  <label class="add" class:disabled={busy}>
    <Icon name="plus" size={14} /> Add pages from a PDF
    <input type="file" accept="application/pdf,.pdf" disabled={busy} onchange={merge} />
  </label>
  <!-- Keyed by document so thumbnails redraw after every edit. -->
  {#key doc}
    <ol>
      {#each indices as i (i)}
        <li class:current={current === i + 1}>
          <button class="thumb" title="Go to page {i + 1}" onclick={() => ongo(i + 1)}>
            <canvas use:thumb={i}></canvas>
          </button>
          <div class="actions">
            <span class="num">{i + 1}</span>
            <button disabled={busy} title="Rotate left" onclick={() => onop({ kind: 'rotate', index: i, by: -90 })}><Icon name="undo" size={13} /></button>
            <button disabled={busy} title="Rotate right" onclick={() => onop({ kind: 'rotate', index: i, by: 90 })}><Icon name="redo" size={13} /></button>
            <button disabled={busy || i === 0} title="Move up" onclick={() => onop({ kind: 'move', from: i, to: i - 1 })}><Icon name="arrowUp" size={13} /></button>
            <button disabled={busy || i === indices.length - 1} title="Move down" onclick={() => onop({ kind: 'move', from: i, to: i + 1 })}><Icon name="arrowDown" size={13} /></button>
            <button disabled={busy} title="Insert a blank page after" onclick={() => onop({ kind: 'blank', after: i })}><Icon name="plus" size={13} /></button>
            <button disabled={busy || indices.length === 1} title="Delete page" onclick={() => onop({ kind: 'delete', index: i })}><Icon name="trash" size={13} /></button>
          </div>
        </li>
      {/each}
    </ol>
  {/key}
</aside>

<style>
  aside {
    position: absolute;
    inset: 0 auto 0 0;
    width: 200px;
    box-sizing: border-box;
    overflow-y: auto;
    padding: 10px;
    background: var(--bar);
    border-right: 1px solid var(--line);
  }
  .add {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 7px;
    border-radius: 6px;
    background: var(--field);
    cursor: pointer;
  }
  .add:focus-within {
    outline: 2px solid var(--accent);
  }
  .add.disabled {
    opacity: 0.45;
    cursor: default;
  }
  .add input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: inherit;
  }
  ol {
    list-style: none;
    margin: 10px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .thumb {
    display: block;
    width: 100%;
    padding: 4px;
    border: 2px solid transparent;
  }
  .current .thumb {
    border-color: var(--accent);
  }
  canvas {
    display: block;
    width: 100%;
    min-height: 60px;
    background: #fff;
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.25);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 1px;
  }
  .num {
    flex: 1;
    color: var(--muted);
    font-size: 12px;
    padding-left: 4px;
  }
  .actions button {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
  }
</style>
