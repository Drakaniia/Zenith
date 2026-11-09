<!-- Prompts, popovers, dialogs and menus mounted once at the root of the window. -->
<script lang="ts">
  import Glance from '$features/browser/Glance.svelte';
  import PermissionPrompt from '$features/page/PermissionPrompt.svelte';
  import PasswordSavePrompt from '$features/passwords/PasswordSavePrompt.svelte';
  import CardSavePrompt from '$features/passwords/cards/CardSavePrompt.svelte';
  import PasswordSuggest from '$features/passwords/PasswordSuggest.svelte';
  import ExtensionInstallPrompt from '$features/extensions/ExtensionInstallPrompt.svelte';
  import DownloadsPopover from '$features/downloads/DownloadsPopover.svelte';
  import ExtensionsPopover from '$features/extensions/ExtensionsPopover.svelte';
  import MediaPanel from '$features/downloads/MediaPanel.svelte';
  import SpaceEditor from '$features/sidebar/SpaceEditor.svelte';
  import BookmarkPopup from '$features/bookmarks/BookmarkPopup.svelte';
  import BookmarkDialog from '$features/bookmarks/BookmarkDialog.svelte';
  import BookmarkImport from '$features/bookmarks/BookmarkImport.svelte';
  import ProfileDialog from '$features/profiles/ProfileDialog.svelte';
  import Palette from '$features/palette/Palette.svelte';
  import Menu from './Menu.svelte';
  import BookmarkGhost from '$features/bookmarks/BookmarkGhost.svelte';
  import Toasts from './Toasts.svelte';
  import DownloadFlights from '$features/downloads/DownloadFlights.svelte';
</script>

<Glance />
<PermissionPrompt />
<PasswordSavePrompt />
<CardSavePrompt />
<PasswordSuggest />
<ExtensionInstallPrompt />
<DownloadsPopover />
<ExtensionsPopover />
<MediaPanel />
<SpaceEditor />
<BookmarkPopup />
<BookmarkDialog />
<BookmarkImport />
<ProfileDialog />
<Palette />
<Menu />
<BookmarkGhost />
<Toasts />
<DownloadFlights />
