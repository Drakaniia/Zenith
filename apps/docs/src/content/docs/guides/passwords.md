---
title: Passwords and payment cards
description: Save, fill and manage passwords and payment cards in Zenith's encrypted vault, protected by a master password.
---

Zenith has a built-in password manager and wallet. Passwords and cards are stored encrypted on your PC and never leave it. Open them from the app menu with **Passwords**, or from the command palette. The page has four sections: **Wallet & cards**, **Passwords**, **Personal data** (not available yet) and **Autofill settings**.

## The master password

The first time you save a password or open the vault, Zenith asks you to set a master password (at least 8 characters). It's never stored: Zenith uses it to unlock the key that encrypts your passwords and cards.

You also get a **recovery key**, a code in the form `XXXX-XXXX-…`. Keep it somewhere safe. If you forget your master password, choose **Forgot your master password?** and enter the recovery key to set a new one. Without either, the only way forward is to delete your saved passwords and start over.

While the vault is locked, Zenith can still show which accounts and cards you have for a site, but it needs the master password to fill in, show or copy a password or card number. The vault locks:

- after a period without using a password (15 minutes by default; choose 1 minute to 4 hours, or only when Zenith closes), and
- always when Zenith closes or restarts.

**Lock now** is in the menu on the Passwords page and in **Autofill settings**.

## Saving and filling passwords

After you sign in to a site, Zenith offers to save the account. Choose **Never** to stop it asking for that site; you can undo that under **Autofill settings › Never saved for**. Incognito windows never offer to save.

Click a username or password field to pick one of your accounts for that site. On sign-up and change-password forms, Zenith can suggest a strong random password.

Zenith only fills a password on the site it was saved for, and never fills a password saved for an HTTPS site into an insecure page.

## Managing passwords

On the **Passwords** page you can search, view, copy, edit and delete entries. The menu on that page also has:

- **Import passwords…**: a CSV export from Chrome, Edge, Brave, Opera, Vivaldi, Firefox, Safari, Bitwarden, 1Password, LastPass, Dashlane, Proton Pass or KeePass/KeePassXC, or a Zenith backup. A preview shows what each row will do; for accounts you already have with a different password, you choose which to keep.
- **Back up (encrypted)…**: a file encrypted with your current master password. Restore it with Import; you'll need the master password you had when you made it.
- **Export as CSV (not encrypted)…**: in Chrome's format, which other password managers can import. Anyone who can open the file can read every password, so delete it once you've imported it elsewhere.

## Payment cards

Zenith can save cards when you pay on a site. It asks first, and only saves the security code if you tick the box. You can also add and edit cards under **Wallet & cards**.

To fill a card, click a card number field on a checkout form and pick a card. Zenith asks for your master password every time it fills a card, and only fills cards on secure (`https`) pages.

## Autofill settings

| Setting | Default |
| --- | --- |
| **Password manager**: turning it off stops saving, suggestions and filling, but keeps your saved passwords | On |
| **Offer to save passwords** | On |
| **Suggest strong passwords** | On |
| **Offer to save cards** | On |
| **Show saved cards on payment forms** | On |
| **Show saved accounts on login fields** | On |
| **Fill usernames and emails** | On |
| **Fill passwords**: off fills only the username | On |
| **When to fill**: *When I pick an account*, or *Automatically* when you have exactly one account for that exact site and the vault is unlocked | When I pick an account |
| **Use on other parts of the same site**: for example an accounts.google.com password on mail.google.com, never across different sites | On |
| **Ask for the master password**: *When locked*, or *Every time* a password is filled, shown or copied | When locked |
| **Lock passwords** | After 15 minutes |

The same section lets you change the master password, make a new recovery key (the old one stops working), and **Delete all saved passwords**.

## How it's protected

- Passwords, notes and card numbers are encrypted with AES-256-GCM using a random vault key. That key is stored only wrapped with keys derived from your master password (Argon2id) and from your recovery key.
- Site names, usernames and card summaries (brand, last four digits, expiry) are encrypted with Windows DPAPI, so they're readable only by your Windows account. That's what lets Zenith suggest accounts before you unlock.
- Each [profile](../profiles/) has its own vault.
