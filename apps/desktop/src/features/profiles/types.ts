/** A browser profile (see profiles.rs). Each one runs in its own Zenith process. */
export interface ProfileInfo {
  id: string;
  name: string;
  /** `#rrggbb`. */
  color: string;
  isDefault: boolean;
  /** `image`, `emoji:<name>`, or absent for the initial on the colour. */
  avatar?: string | null;
  /** The chosen picture as a `data:image/png` URL (only for the `image` avatar). */
  image?: string | null;
}

export interface ProfileEntry extends ProfileInfo {
  /** A Zenith process has it open (the calling one included). */
  running: boolean;
  /** Zenith's desktop shortcut for it exists. */
  shortcut: boolean;
}

export interface ProfilesView {
  /** The calling window's profile. */
  active: string;
  profiles: ProfileEntry[];
  /** Colours offered for profiles. */
  colors: string[];
}
