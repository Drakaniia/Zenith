<script lang="ts">
  let {
    checked = $bindable(false),
    label = '',
    disabled = false,
    onchange,
  }: { checked?: boolean; label?: string; disabled?: boolean; onchange?: (v: boolean) => void } = $props();
</script>

<button
  type="button"
  class="toggle"
  class:on={checked}
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  onclick={() => {
    checked = !checked;
    onchange?.(checked);
  }}
>
  <span class="knob"></span>
</button>

<style>
  .toggle {
    position: relative;
    width: 40px;
    height: 22px;
    flex: none;
    border-radius: 11px;
    background: var(--field);
    border: 1px solid var(--line-strong);
    transition: background var(--fast) var(--ease), border-color var(--fast);
  }
  .toggle.on {
    background: var(--accent);
    border-color: transparent;
  }
  .toggle:disabled {
    opacity: 0.45;
  }
  .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--text-2);
    transition: transform var(--med) var(--ease-out), background var(--fast), width var(--fast);
  }
  .toggle.on .knob {
    transform: translateX(18px);
    background: #fff;
  }
  .toggle:active .knob {
    width: 17px;
  }
  .toggle.on:active .knob {
    transform: translateX(15px);
  }
</style>
