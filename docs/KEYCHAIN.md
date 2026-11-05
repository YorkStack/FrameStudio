# macOS Keychain troubleshooting

*Deutsch: [de/KEYCHAIN.md](de/KEYCHAIN.md)*

FrameStudio stores persistent provider keys as generic app entries in the macOS login keychain. This page explains where to find them and what to do when the app reports that a stored key cannot be read.

## Where the entries are

| Keychain | Service | Account |
| --- | --- | --- |
| Login (`login.keychain-db`) | `com.yorkstack.framestudio` | `gemini-api-key`, `openai-api-key`, `anthropic-api-key` |

Open **Keychain Access**, choose **Login → All Items** and search for `com.yorkstack.framestudio`. The **Passwords** app does not show these generic app entries like website passwords. Entries saved by current versions have German display names such as `FrameStudio · Google Gemini API-Schlüssel`; service and account stay compatible with older entries.

## Why a stored key can become unreadable

A stored-key indicator means the entry exists; it does not prove that the current app build may read it. The published macOS build is **ad-hoc signed**. Its code signature is bound to the exact binary hash, so every new build or update has a different identity. The keychain may therefore deny a newer build access to an entry created by an older one (error -25293, authorization denied). FrameStudio never changes keychain access lists and never opens keychain dialogs automatically.

## Authorize one specific build

1. Let running jobs finish and decide which FrameStudio app you actually use.
2. In **Keychain Access**, open the matching account, go to **Access Control**, add or allow only this app and confirm the change with your keychain password. Do not allow access for all applications.
3. In FrameStudio, choose **Check keychain only**. This test reads from the OS store even while a session key is active and sends nothing to the provider.

An authorization for an ad-hoc signed build does not carry over to future builds. Alternatively:

- delete the old entry and save the key again from the current build, or
- use **This session only · do not save**, which never touches the keychain.

## Permanent solution for updates

Builds signed with the same stable code-signing identity and a matching designated requirement keep keychain access across updates; regular distribution would use a Developer ID and notarization. Until then, session keys remain available.

Apple sources, checked 2026-10-31:

- [TN3127: Inside Code Signing: Requirements](https://developer.apple.com/documentation/technotes/tn3127-inside-code-signing-requirements)
- [Understanding the Code Signature](https://developer.apple.com/library/archive/documentation/Security/Conceptual/CodeSigningGuide/AboutCS/AboutCS.html)

## How saving is verified

After a persistent save, FrameStudio reads the key back and compares it before reporting success. If that check fails after a confirmed write, the app says explicitly that the key may have been stored. There is no plaintext fallback and no automatic password dialog.
