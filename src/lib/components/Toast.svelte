<script lang="ts">
  import { CheckCircle2, Info, X, XCircle } from 'lucide-svelte';
  import { toastsState, type ToastKind } from '../state/toasts.svelte';

  function labelFor(kind: ToastKind): string {
    if (kind === 'success') {
      return 'Success';
    }

    if (kind === 'error') {
      return 'Error';
    }

    return 'Info';
  }
</script>

<div class="toast-region" aria-live="polite" aria-relevant="additions removals">
  {#each toastsState.items as toast (toast.id)}
    <article
      class={`toast toast--${toast.kind}`}
      role={toast.kind === 'error' ? 'alert' : 'status'}
    >
      <div class="toast__icon" aria-hidden="true">
        {#if toast.kind === 'success'}
          <CheckCircle2 size={18} strokeWidth={2.25} />
        {:else if toast.kind === 'error'}
          <XCircle size={18} strokeWidth={2.25} />
        {:else}
          <Info size={18} strokeWidth={2.25} />
        {/if}
      </div>

      <div class="toast__body">
        <div class="toast__label">{labelFor(toast.kind)}</div>
        <div class="toast__message">{toast.message}</div>
      </div>

      <button
        class="toast__close"
        type="button"
        aria-label="Dismiss notification"
        onclick={() => toastsState.remove(toast.id)}
      >
        <X size={16} strokeWidth={2.25} />
      </button>
    </article>
  {/each}
</div>

<style>
  .toast-region {
    position: fixed;
    z-index: 1000;
    top: 16px;
    right: 16px;
    display: grid;
    gap: 10px;
    width: min(360px, calc(100vw - 32px));
    pointer-events: none;
  }

  .toast {
    display: grid;
    grid-template-columns: 24px minmax(0, 1fr) 28px;
    gap: 10px;
    align-items: start;
    min-height: 56px;
    padding: 10px;
    pointer-events: auto;
    color: var(--color-text-primary);
    background: color-mix(in srgb, var(--color-surface-elevated) 94%, transparent);
    border: 1px solid var(--color-border);
    border-left: 3px solid var(--toast-accent);
    border-radius: var(--radius-control);
    box-shadow: var(--shadow-elevated);
    backdrop-filter: blur(18px);
    animation: toast-in 160ms ease-out;
  }

  .toast--success {
    --toast-accent: var(--color-success);
  }

  .toast--error {
    --toast-accent: var(--color-danger);
  }

  .toast--info {
    --toast-accent: var(--color-accent);
  }

  .toast__icon {
    display: grid;
    width: 24px;
    height: 24px;
    place-items: center;
    color: var(--toast-accent);
  }

  .toast__body {
    min-width: 0;
  }

  .toast__label {
    margin-bottom: 2px;
    color: var(--color-text-primary);
    font-size: 12px;
    font-weight: 650;
    line-height: 1.2;
  }

  .toast__message {
    color: var(--color-text-secondary);
    font-size: 13px;
    line-height: 1.35;
    overflow-wrap: anywhere;
  }

  .toast__close {
    display: grid;
    width: 28px;
    height: 28px;
    place-items: center;
    padding: 0;
    color: var(--color-text-secondary);
    background: transparent;
    border: 0;
    border-radius: 6px;
    cursor: pointer;
  }

  .toast__close:hover {
    color: var(--color-text-primary);
    background: rgb(255 255 255 / 0.08);
  }

  :global(.app-shell[data-theme='light']) .toast__close:hover {
    background: rgb(0 0 0 / 0.06);
  }

  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(-6px);
    }

    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
