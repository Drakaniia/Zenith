<script lang="ts">
  import { avatarFace } from '../lib/profile-picture';
  import { textOn } from '../lib/profiles';

  // A profile's round avatar: its chosen picture, a bundled emoji on its colour, or its initial.
  let {
    name,
    color,
    avatar = null,
    image = null,
    size = 26,
    ring = false,
  }: { name: string; color: string; avatar?: string | null; image?: string | null; size?: number; ring?: boolean } = $props();

  const face = $derived(avatarFace({ name, avatar, image }));
</script>

<span
  class="avatar"
  class:ring
  class:photo={face.kind === 'image'}
  style:--size={`${size}px`}
  style:background={face.kind === 'image' ? 'transparent' : color}
  style:color={textOn(color)}
  aria-hidden="true"
>
  {#if face.kind === 'letter'}
    {face.text}
  {:else}
    <img class:emoji={face.kind === 'emoji'} src={face.src} alt="" draggable="false" />
  {/if}
</span>

<style>
  .avatar {
    width: var(--size);
    height: var(--size);
    flex: none;
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: 50%;
    font-size: max(10.5px, calc(var(--size) * 0.46));
    font-weight: 700;
    line-height: 1;
    user-select: none;
  }
  .ring {
    box-shadow: 0 0 0 1px color-mix(in srgb, currentColor 18%, transparent);
  }
  .photo.ring {
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--text) 14%, transparent);
  }
  img {
    width: 100%;
    height: 100%;
    display: block;
    object-fit: cover;
    pointer-events: none;
  }
  img.emoji {
    width: 62%;
    height: 62%;
    object-fit: contain;
  }
</style>
