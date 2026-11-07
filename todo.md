neuroprotect block malicous sites

translucent ui feature

code-signing certificate for the installer

load design skill make website for showcaseing the browser, you may take screenshot when running the app.. (the browser should be minimalist similar to zen browser website)

create AGENTS.md

when i ctrl + l to search, when i tab for the result,  the link should be

zenith profile, can switch different browser profile

notes page tool

similarly when click tab for the results, it auto complete also the site address  
purpose for faster search

feat: can change image of the 

currently the tableu is not customizable, no tooltiip when i right click, i cant edit the link and name of tabs. currenlty its recent tabs, but i want to make it saved  just like bookmarks for faster navigation  along with the search pill..

also remove the logo,  replace it like wather or time of both

saved password (when i)

save password feature: when i enter password to  website like facebook or google or any site that uses password, will be saved (also there is popup master password input), so when i revisit that site again - it will autocomplete the profile like username , name, and when that is usesd, add a popup somewhere that enter the master password - will now auto complete the login with the saved password of that email or any details 

# Password Manager, Master Password, and Autofill Feature

Implement a complete and secure password management system in the Zenith browser, inspired by the password management interface shown in the provided screenshots.

## 1. Save Password Feature

Whenever I log in to a website, such as Facebook, Google, or any other website that requires credentials:

- Automatically detect login forms.
- Offer to save the username, email, password, and other relevant login details.
- Allow me to save credentials securely using my master password.
- Associate saved credentials with their respective websites.
- Support multiple accounts for the same website.

When I revisit a website, Zenith should recognize the saved credentials and display an autofill suggestion containing the saved account's username, email, or profile name.

When I select a saved account, display a popup requesting my master password if authentication is required. After successful authentication, automatically fill in the saved login credentials, including the password.

## 2. Password Manager Page

Create a dedicated Password Manager page accessible through the browser's settings.

The interface should resemble the provided screenshots, featuring:

- A list of all saved passwords and accounts.
- Website name and favicon.
- Username or login information.
- Password information.
- Search functionality to quickly find saved accounts.
- Options to add, edit, and delete saved passwords.
- Support for multiple accounts per website.
- A clean, organized interface consistent with Zenith's existing design.

### Master Password Protection

The Password Manager page must be protected by the master password.

- When opening the Password Manager, require the user to enter their master password before accessing saved credentials.
- Do not allow users to view, add, edit, delete, or manage saved passwords without successful authentication.
- Provide an option to lock the Password Manager manually.
- Automatically lock access when the configured security timeout expires or when the browser is restarted, depending on the user's security settings.

### Forgot Master Password

Include a **Forgot Master Password** option alongside the master password input.

When selected, provide a secure password recovery or reset workflow.

If the master password cannot be recovered, clearly explain that encrypted passwords may need to be deleted before creating a new master password. Never silently delete saved credentials.

## 3. Add and Manage Passwords

Allow users to manually add new password entries.

Each entry should support:

- Website URL or domain.
- Username or email.
- Password.
- Optional account name or profile label.
- Optional notes.

Users should be able to view, edit, copy, update, and delete saved credentials after successfully entering their master password.

Password visibility should be hidden by default, with an option to reveal the password after authentication.

## 4. Import and Export Passwords

Add password import and backup functionality.

### Import Passwords
- Allow users to import passwords from supported browsers or password managers.
- Support CSV import where appropriate.
- Provide a preview of imported credentials before saving.
- Detect duplicate entries and allow users to manage conflicts.

### Backup and Export Passwords
- Allow users to create backups of their saved passwords.
- Support exporting credentials in a compatible format, such as CSV, when explicitly requested.
- Require master password authentication before exporting sensitive information.
- Clearly warn users that exported files may contain unencrypted credentials.
- Provide a secure backup option that encrypts the backup using the master password.

## 5. Password Manager Settings and Autofill

Create a dedicated section in the browser settings for Password Manager and Autofill, similar to the settings shown in the screenshots.

Include the following options:

### Password Settings
- Enable or disable saving passwords by default.
- Enable or disable automatic login form filling.
- Choose whether to autofill usernames only or both usernames and passwords.
- Enable or disable password suggestions.
- Configure when the master password is required to access saved credentials.
- Configure automatic locking after inactivity or browser restart.
- Enable or disable the Password Manager.

### Autofill Settings
- Enable or disable automatic username and email filling.
- Enable or disable automatic password filling.
- Allow users to choose whether credentials are filled automatically or only after selecting a saved profile.
- Show saved account suggestions when interacting with login fields.
- Support websites with multiple login accounts.
- Prevent autofill on websites that do not match the saved credential's domain.
- Allow users to manage autofill behavior and saved personal information.

## 6. Security Requirements

Security is a priority for this feature.

- Encrypt saved passwords and sensitive credentials at rest.
- Use the master password to protect access to encrypted credentials.
- Never store the master password in plaintext.
- Require authentication before revealing or exporting passwords.
- Prevent unauthorized access to saved credentials.
- Avoid exposing passwords in logs, debugging output, or browser UI unnecessarily.
- Ensure that password autofill only occurs on the correct website.
- Clearly communicate the consequences of forgetting or resetting the master password.

## 7. UI and Design

Use the provided screenshots as the primary reference for the Password Manager and Autofill Settings interfaces.

Maintain Zenith's existing dark theme, layout, typography, and visual consistency.

The Password Manager should include a sidebar with sections such as:

- Wallet and Cards
- Passwords
- Personal Data
- Autofill Settings

The Passwords page should display saved credentials in an organized list, with search, add, edit, and management options.

The Autofill Settings page should provide clear toggles, dropdowns, and controls for managing password storage, master password protection, and autofill behavior.

Ensure the entire system feels native to Zenith and works seamlessly with the browser's existing functionality.

**Goal:** Build a complete, secure, user-friendly password manager for Zenith that supports password saving, master password protection, credential management, importing, backups, and customizable autofill settings.




# Zenith Notes / Text Editor Feature

Implement a built-in **Notes and Text Editor** feature in the Zenith browser, inspired by Yandex Browser

## 1. Notes and Text Editor

Create a dedicated Notes or Editor page that allows users to quickly write, edit, and manage notes directly within Zenith without needing an external application.

## 2. Notes Management

Users should be able to:

- Create new notes instantly.
- Edit existing notes.
- Automatically save changes.
- Rename notes.
- Delete notes.
- Search through saved notes.
- View recently edited notes in the sidebar.
- Display the last modified date or time.
- Switch between different notes without losing unsaved changes.

Each note should have its own independent content and title.

## 3. Editor Toolbar

Include useful editing tools similar to those shown in the screenshot:

- Create new note.
- Undo and redo.
- Copy note content to the clipboard.
- Toggle the editor layout or sidebar.

Ensure these actions work correctly and integrate with Zenith's existing functionality where applicable.

## 4. Keyboard Shortcut

Allow users to open the Notes Editor using a customizable keyboard shortcut.

**Ask the user which keyboard shortcut they want to use for this feature before implementing the final shortcut.**

The shortcut should:

- Open the Notes Editor from anywhere in the browser.
- Work even when switching between tabs, applications, or desktop windows.
- Avoid conflicts with existing browser shortcuts.
- Allow users to change the shortcut through Zenith's settings.
- Support a default shortcut if the user does not specify one.

## 5. Navigation and Accessibility

The Notes Editor should be accessible through:

- Zenith's main navigation or sidebar.
- Browser settings or tools menu.
- The configured keyboard shortcut.

Ensure the editor opens reliably and retains its state when switching between browser tabs or windows.

## 6. UI and Design

Use the provided screenshot as the primary visual reference.

Maintain the same overall layout:

- Left sidebar for note history and search.
- Main editor workspace.
- Top toolbar with editing actions.
- Dark theme with consistent spacing, typography, and controls.

Make the editor responsive and ensure it works properly across different window sizes.

**Goal:** Build a native, convenient, and fully functional Notes Editor within Zenith, allowing users to create, manage, and edit notes quickly using both the interface and a customizable keyboard shortcut.
