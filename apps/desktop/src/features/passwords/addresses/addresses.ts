// Saved addresses: the type shared with the backend (vault/addresses.rs, address_commands.rs) and
// pure helpers for the UI. Kept free of Svelte and Tauri imports so it can be unit tested.

export interface SavedAddress {
  id: string;
  name: string;
  organization: string;
  /** One line per street line. */
  street: string;
  city: string;
  /** State, province or region. */
  region: string;
  postal: string;
  country: string;
  phone: string;
  email: string;
  created: number;
  updated: number;
}

export function emptyAddress(): SavedAddress {
  return { id: "", name: "", organization: "", street: "", city: "", region: "", postal: "", country: "", phone: "", email: "", created: 0, updated: 0 };
}

/** What the address is called in lists: the name, else the organization, street or email. */
export function addressTitle(a: SavedAddress): string {
  return a.name || a.organization || a.street.split("\n")[0] || a.email || a.phone || "Address";
}

/** "1 Main St, Apt 4, Springfield, IL 62701, US" */
export function addressLine(a: SavedAddress): string {
  const regionPostal = [a.region, a.postal].filter(Boolean).join(" ");
  return [...a.street.split("\n"), a.city, regionPostal, a.country].map((s) => s.trim()).filter(Boolean).join(", ");
}
