import { params } from "./fixtures";
import { UNHANDLED } from "./handler";

// Password vault for zenith://passwords, kept in memory. ?vault=setup|locked (default unlocked).
const vaultParam = params.get("vault");
const previewVault = {
  exists: vaultParam !== "setup",
  unlocked: !vaultParam,
  entries: [
    ["https://accounts.google.com", "ann.lee@gmail.com", "Personal", "Tr0ub4dor&3-horse"],
    ["https://accounts.google.com", "ann@work.example", "Work", "x7#Qm2!vR9pL"],
    ["https://www.facebook.com", "ann.lee@gmail.com", "", "sunflower-Bicycle-42"],
    ["https://github.com", "annlee", "", "gh-9f!Kq2Lz"],
    ["https://www.netflix.com", "ann.lee@gmail.com", "Family", "popcorn&Couch7"],
    ["https://login.microsoftonline.com", "ann@work.example", "Work", "Spring2026!Team"],
  ].map(([origin, username, label, password], i) => ({
    id: `v${i}`,
    origin,
    site: new URL(origin).hostname.replace(/^www\./, ""),
    username,
    label,
    password,
    created: Date.now() - (i + 2) * 86_400_000,
    updated: Date.now() - (i + 2) * 86_400_000,
    lastUsed: i % 2 ? Date.now() - i * 3_600_000 : 0,
    timesUsed: i,
    hasNotes: i === 1,
  })),
};

// Saved cards for the Wallet section (the preview master password is "preview").
let previewCards = [
  { id: "c0", label: "Personal", holder: "Ann Lee", brand: "visa", last4: "4242", expMonth: 8, expYear: 2031, hasCvc: false, number: "4242424242424242", cvc: "" },
  { id: "c1", label: "Work", holder: "Ann Lee", brand: "amex", last4: "0005", expMonth: 11, expYear: 2028, hasCvc: true, number: "378282246310005", cvc: "1234" },
].map((c, i) => ({ ...c, created: Date.now() - (i + 3) * 86_400_000, updated: Date.now() - (i + 3) * 86_400_000, lastUsed: i ? Date.now() - 3_600_000 : 0, timesUsed: i }));

function vaultStatus() {
  return {
    exists: previewVault.exists,
    unlocked: previewVault.unlocked,
    hasRecovery: true,
    count: previewVault.exists ? previewVault.entries.length : 0,
    cardCount: previewVault.exists ? previewCards.length : 0,
    damaged: null,
    retryAfter: 0,
  };
}

/** Password vault and wallet (cards) commands. */
export function handlePasswords(cmd: string, a: Record<string, unknown>): unknown {
  switch (cmd) {
    case "vault_status":
      return vaultStatus();
    case "vault_setup":
      previewVault.exists = previewVault.unlocked = true;
      return "8K2D-Q4M7-ZP3X-T9WA-H6RC-1VJN-5BEG-YF0S";
    case "vault_unlock":
      if (a.master !== "preview") return Promise.reject("That master password isn't right.");
      previewVault.unlocked = true;
      return vaultStatus();
    case "vault_lock":
      previewVault.unlocked = false;
      return null;
    case "passwords_list":
      if (!previewVault.unlocked) return Promise.reject("The password manager is locked.");
      return previewVault.entries.map(({ password: _, ...e }) => e);
    case "password_reveal": {
      const e = previewVault.entries.find((x) => x.id === a.id);
      return { password: e?.password ?? "", notes: e?.hasNotes ? "Recovery codes are in the safe." : "" };
    }
    case "password_generate":
      return "pR7-kQz!m2Vx9Lw_4hTc";
    case "passwords_never_list":
      return ["bank.example"];
    case "passwords_delete": {
      const ids = a.ids as string[];
      previewVault.entries = previewVault.entries.filter((x) => !ids.includes(x.id));
      return ids.length;
    }
    case "cards_list":
      if (!previewVault.unlocked) return Promise.reject("The password manager is locked.");
      return previewCards.map(({ number: _n, cvc: _c, ...c }) => c);
    case "card_reveal": {
      if (a.master !== "preview") return Promise.reject(a.master ? "That master password isn't right." : "Enter your master password to continue.");
      const c = previewCards.find((x) => x.id === a.id);
      return { number: c?.number ?? "", cvc: c?.cvc ?? "" };
    }
    case "card_copy":
      if (a.field !== "holder" && a.master !== "preview") return Promise.reject(a.master ? "That master password isn't right." : "Enter your master password to continue.");
      return null;
    case "card_add": {
      const c = a.card as { number: string; holder: string; expMonth: number; expYear: number; label: string; cvc: string | null };
      const n = (c.number ?? "").replace(/\D/g, "");
      previewCards = [
        ...previewCards,
        { id: `c${Date.now()}`, label: c.label, holder: c.holder, brand: n.startsWith("4") ? "visa" : "card", last4: n.slice(-4), expMonth: c.expMonth, expYear: c.expYear < 100 ? 2000 + c.expYear : c.expYear, hasCvc: !!c.cvc, number: n, cvc: c.cvc ?? "", created: Date.now(), updated: Date.now(), lastUsed: 0, timesUsed: 0 },
      ];
      return previewCards[previewCards.length - 1].id;
    }
    case "card_update": {
      const c = a.card as { holder: string; expMonth: number; expYear: number; label: string };
      previewCards = previewCards.map((x) => (x.id === a.id ? { ...x, holder: c.holder, expMonth: c.expMonth, expYear: c.expYear, label: c.label } : x));
      return null;
    }
    case "cards_delete": {
      const ids = a.ids as string[];
      previewCards = previewCards.filter((x) => !ids.includes(x.id));
      return ids.length;
    }
    default:
      return UNHANDLED;
  }
}
