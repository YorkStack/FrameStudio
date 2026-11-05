# Security design

*Deutsch: [de/SECURITY-DESIGN.md](de/SECURITY-DESIGN.md) · How to report a vulnerability: [SECURITY.md](../SECURITY.md)*

This document describes the security-relevant design decisions. The general trust model (local OS account, credential store, REST/MCP boundaries) is in [ARCHITECTURE.md](ARCHITECTURE.md).

## Series approvals

`series.submit` stores an immutable, concrete image list with prompts, reference identities and price basis. The origin is determined on the server side from the transport and the authenticated token record. No key or token is stored in the series record. Names and recipients are local labels, not verified identities; whoever shares a token shares its origin label. REST/MCP cannot approve series or change the general spending approval. Only the local GUI or trusted CLI may approve or reject; budget check and job creation happen together atomically. There is no self-approval through client-controlled fields or imported project packages.

Idempotency is bound per access and transport to the unchanged request. Price changes before approval require resubmission; later style changes do not alter frozen prompts. On start, all estimates are reserved together. Changes, retries and additional images are new requests. Revoking a token does not retroactively cancel existing series approvals; cancel series separately. At most 32 expanded images and 100 pending approvals limit accidental explosions. With unknown prices, nothing starts without a reliable reservation.

If a session key is active at approval time, the related jobs are marked `session_key_required`. A worker without a session key for that provider does not claim them and does not query the OS store for them. The marker is not a secret; credentials stay in their own process. After a restart the key must be entered again, but the series does not need to be approved again. A session key for the same provider that is deliberately set in another local process is not an isolated user space; FrameStudio continues to trust the local OS user.

## Key access and session keys

`state.key_status` reports only existing keychain metadata; "configured" is no promise of read permission or provider validity. `secret.check` is an explicitly triggered local read check, run before GUI generation and via the **Check key access** button. It returns only `provider`, `readable` and `provider_contacted: false`, never the secret. REST and MCP may not call this operation. No background check reads keys for the status display.

The macOS errors -25308 (interaction required), -25293 (authorization denied), -25291 (service not available) and -128 (cancelled) are reported separately. An unlocked keychain does not guarantee that an existing entry is authorized for a newly built app binary. FrameStudio never opens keychain dialogs automatically and never changes access lists. The **Passwords** app does not replace this app authorization in **Keychain Access**. Sources: [Apple: app access](https://support.apple.com/en-gb/guide/mac-help/kychn002/26/mac/26), [Apple: Access Control Lists](https://developer.apple.com/documentation/security/access-control-lists), checked 2026-10-31. Practical steps: [KEYCHAIN.md](KEYCHAIN.md).

Ordinary image and prompt jobs are also stored as `session_key_required` while a session key is active, not only approved series. A worker without a session key for the provider does not claim these jobs and does not fall back to the OS store. After an app restart the user must enter the session key again. The local GUI check before queuing does not replace provider validation; key access can change between check and processing.

## Public distribution (0.3.0)

Source and releases are prepared from an explicit file allowlist, without local databases, personal images, token records, private example ZIPs or development history. Public binaries remap local compiler paths. Included style thumbnails have no text or EXIF chunks containing prompts.

The native folder chooser selects a path only. Applying the workspace change validates absolute paths, symlink boundaries, directory contents and writability. The old workspace is preserved; selection never silently copies or erases it.

The key creation help opens only these fixed official destinations:

- https://aistudio.google.com/api-keys
- https://platform.openai.com/api-keys
- https://platform.claude.com/settings/keys

These destinations were checked against the official provider documentation on 2026-10-31. No provider key is ever inserted into a URL.
