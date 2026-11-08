<script lang="ts">
  // The space's colour tinting the window (Zen's workspace gradient), cross-fading on a switch.
  import { fade } from 'svelte/transition';
  import { springEase } from '$shared/ui/motion';
  import type { Space } from '$features/browser/types';

  let { space }: { space: Space | undefined } = $props();
</script>

{#key space?.id}
  <div
    class="space-bg"
    style:--space-c={space?.color ?? '#8b7cf6'}
    in:fade={{ duration: 250, easing: springEase }}
    out:fade={{ duration: 250, easing: springEase }}
  ></div>
{/key}

<style>
  /* --space-c is a registered colour (app.css), so changing the space's accent fades the tint
   * instead of switching it. */
  .space-bg {
    position: absolute;
    inset: 0;
    transition: --space-c 300ms ease;
    background:
      radial-gradient(120% 90% at 0% 0%, color-mix(in srgb, var(--space-c) 24%, transparent), transparent 70%),
      linear-gradient(160deg, color-mix(in srgb, var(--space-c) 16%, transparent), color-mix(in srgb, var(--space-c) 7%, transparent));
  }
  :global(:root[data-theme='light']) .space-bg {
    background:
      radial-gradient(120% 90% at 0% 0%, color-mix(in srgb, var(--space-c) 30%, transparent), transparent 70%),
      linear-gradient(160deg, color-mix(in srgb, var(--space-c) 20%, transparent), color-mix(in srgb, var(--space-c) 9%, transparent));
  }
</style>
