<script module lang="ts">
  export interface Signature {
    png: Uint8Array;
    width: number;
    height: number;
  }
</script>

<script lang="ts">
  import { onMount } from 'svelte';

  let { ondone }: { ondone: (sig: Signature | null) => void } = $props();

  const W = 600;
  const H = 200;
  let mode: 'draw' | 'type' = $state('draw');
  let typed = $state('');
  let ink = $state('#1a1a8c');
  let canvas: HTMLCanvasElement;
  let drawn = $state(false);
  let ctx: CanvasRenderingContext2D;

  onMount(() => {
    ctx = canvas.getContext('2d')!;
  });

  function clear() {
    ctx.clearRect(0, 0, W, H);
    drawn = false;
  }

  function point(e: PointerEvent): [number, number] {
    const r = canvas.getBoundingClientRect();
    return [((e.clientX - r.left) * W) / r.width, ((e.clientY - r.top) * H) / r.height];
  }

  function down(e: PointerEvent) {
    if (mode !== 'draw') return;
    canvas.setPointerCapture(e.pointerId);
    ctx.strokeStyle = ink;
    ctx.lineWidth = 3;
    ctx.lineCap = ctx.lineJoin = 'round';
    ctx.beginPath();
    ctx.moveTo(...point(e));
  }

  function move(e: PointerEvent) {
    if (mode !== 'draw' || !canvas.hasPointerCapture(e.pointerId)) return;
    ctx.lineTo(...point(e));
    ctx.stroke();
    drawn = true;
  }

  function render() {
    clear();
    if (!typed.trim()) return;
    ctx.fillStyle = ink;
    ctx.font = `64px 'Segoe Script', 'Brush Script MT', cursive`;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(typed, W / 2, H / 2, W - 20);
    drawn = true;
  }

  /** The signature cropped to its ink. */
  async function done() {
    const { data } = ctx.getImageData(0, 0, W, H);
    let [x0, y0, x1, y1] = [W, H, -1, -1];
    for (let y = 0; y < H; y++)
      for (let x = 0; x < W; x++)
        if (data[(y * W + x) * 4 + 3] > 0) {
          x0 = Math.min(x0, x);
          x1 = Math.max(x1, x);
          y0 = Math.min(y0, y);
          y1 = Math.max(y1, y);
        }
    if (x1 < 0) return ondone(null);
    const pad = 4;
    const w = x1 - x0 + 1 + pad * 2;
    const h = y1 - y0 + 1 + pad * 2;
    const out = Object.assign(document.createElement('canvas'), { width: w, height: h });
    out.getContext('2d')!.drawImage(canvas, x0 - pad, y0 - pad, w, h, 0, 0, w, h);
    const blob = await new Promise<Blob | null>((r) => out.toBlob(r, 'image/png'));
    if (!blob) return ondone(null);
    ondone({ png: new Uint8Array(await blob.arrayBuffer()), width: w, height: h });
  }
</script>

<div class="scrim" role="presentation" onkeydown={(e) => e.key === 'Escape' && ondone(null)}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Signature">
    <div class="head">
      <h2>Signature</h2>
      <div class="seg" role="tablist">
        <button role="tab" aria-selected={mode === 'draw'} class:on={mode === 'draw'} onclick={() => { mode = 'draw'; clear(); }}>Draw</button>
        <button role="tab" aria-selected={mode === 'type'} class:on={mode === 'type'} onclick={() => { mode = 'type'; render(); }}>Type</button>
      </div>
    </div>
    {#if mode === 'type'}
      <!-- svelte-ignore a11y_autofocus -->
      <input placeholder="Your name" autofocus bind:value={typed} oninput={render} />
    {/if}
    <canvas
      bind:this={canvas}
      width={W}
      height={H}
      class:typing={mode === 'type'}
      aria-label="Draw your signature"
      onpointerdown={down}
      onpointermove={move}
    ></canvas>
    <div class="row">
      <label class="ink">Ink <input type="color" bind:value={ink} onchange={() => mode === 'type' && render()} /></label>
      <button class="btn" onclick={clear}>Clear</button>
      <span class="grow"></span>
      <button class="btn" onclick={() => ondone(null)}>Cancel</button>
      <button class="btn primary" disabled={!drawn} onclick={done}>Place</button>
    </div>
  </div>
</div>

<style>
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
    width: min(640px, calc(100vw - 32px));
    padding: 20px;
    border-radius: 12px;
    background: var(--bar);
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.3);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h2 {
    margin: 0;
    font-size: 16px;
  }
  .seg {
    display: flex;
    gap: 2px;
    padding: 2px;
    border-radius: 8px;
    background: var(--field);
  }
  .seg button {
    padding: 5px 12px;
  }
  canvas {
    width: 100%;
    aspect-ratio: 3;
    border: 1px dashed var(--line);
    border-radius: 8px;
    background: #fff;
    touch-action: none;
    cursor: crosshair;
  }
  canvas.typing {
    cursor: default;
  }
  input:not([type='color']) {
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--field);
    color: inherit;
    font: inherit;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .ink {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
  }
  .ink input {
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    background: none;
  }
  .grow {
    flex: 1;
  }
  .btn {
    padding: 6px 12px;
    background: var(--field);
  }
  .btn.primary {
    background: var(--accent);
    color: var(--on-accent);
  }
  .on {
    background: var(--bar) !important;
  }
</style>
