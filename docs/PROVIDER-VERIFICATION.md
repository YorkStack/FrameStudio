# Provider verification

*Deutsch: [de/PROVIDER-VERIFICATION.md](de/PROVIDER-VERIFICATION.md)*

Checked on 2026-10-31. This page records which provider APIs FrameStudio uses, which documentation each integration was verified against, how prices are calculated and what has been tested live.

## Google Gemini

FrameStudio implements `gemini-nano-banana-2.1` (stable) via the documented Gemini Interactions REST API. Calls go only to `https://generativelanguage.googleapis.com/v1beta/interactions`, with `x-goog-api-key` in the header and without redirects. The Rust HTTP client needs no Google SDK. Local test provider: `deterministic-v1`; its test images are not AI generations.

The [official image documentation](https://ai.google.dev/gemini-api/docs/image-generation) describes text and image inputs, image editing and `response_format` with aspect ratio, resolution and MIME type. FrameStudio requests JPEG output without the optional `delivery` field, 1K/2K/4K, ten common aspect ratios and at most eight selected references. These app capabilities are deliberately limited; other currently documented models are not offered as unverified configuration. Seeds, separate negative prompt parameters, transparency, search and other paid tools are not offered. The "Avoid" block is ordinary prompt text. Identity consistency remains uncertain due to the model.

[API reference](https://ai.google.dev/api/interactions-api): `input` contains text and inline image blocks. The response is evaluated via `steps[].content[]`; only verified image bytes are stored. `store:false` avoids additional retrieval storage. Synchronous requests usually have no known provider ID when the connection drops, so no reliable status query is possible. FrameStudio then keeps "result unclear" and the reservation, without retrying. Cancelling stops waiting jobs; requests already sent are read to the end and their results saved.

### Gemini pricing

[Google Developer API pricing](https://ai.google.dev/gemini-api/docs/pricing), standard tier, USD, checked 2026-10-31; snapshot `google-standard-2026-10-31-v1`:

| Usage | USD per million tokens |
|---|---:|
| Text/image input | 1.50 |
| Text and thinking output | 7.50 |
| Image output | 30.00 |

The documented image output tokens are 1120 (1K), 1680 (2K) and 3780 (4K). The upfront estimate adds conservative text, reference and thinking budgets but is not a guaranteed cap. Actual costs are calculated only from complete usage fields including modalities. Missing or contradictory usage stays unknown, including responses without an image. Calculated amounts are not confirmed invoices. Integer micro-USD prevent floating-point errors; sub-units are rounded up. Historical jobs keep their prompt, reference and price snapshot.

### Gemini corrections and live test

- **Output format:** the [Interactions API `ImageResponseFormat`](https://ai.google.dev/api/interactions-api) allows only `image/jpeg` for `mime_type`. The first adapter wrongly sent `image/png`; it now requests JPEG. PNG remains allowed as reference input, and local PNG exports are unaffected. Contract tests now cover the request format.
- **Delivery mode:** an explicitly approved live request returned HTTP 400 "Image delivery mode is not supported." `response_format.delivery` is therefore omitted. The field appears in the generic schema, but the image endpoint rejects it; the official REST image examples do without it. A contract test prevents reintroducing it.
- **Diagnostics:** errors now include the HTTP code, known standardized status codes, Google's sanitized message and field violations. Before anything is stored, the key actually used, the prompt and reference Base64 (including JSON-escaped echoes), typical key/bearer/URL/long-token forms and control characters are removed, and text length is limited. No raw response in logs, no automatic retry.
- **Live result:** after the delivery fix, the same model and photo request returned a valid 1024×1024 JPEG. The reported output tokens (1618) did not match the modality list (1120 image tokens), so the cost stays conservatively unknown. Other accounts, quotas and sizes have not been tested live.

## OpenAI image models

The integration uses the Images API, not the ChatGPT web interface. Verified against the official [image guide](https://developers.openai.com/api/docs/guides/image-generation), [generation reference](https://developers.openai.com/api/reference/resources/images/methods/generate), [edit reference](https://developers.openai.com/api/reference/resources/images/methods/edit) and the model pages for [Sunburst](https://developers.openai.com/api/docs/models/gpt-image-2.5-sunburst) and [Flare](https://developers.openai.com/api/docs/models/gpt-image-2.5-flare).

- Fixed stable snapshots: `gpt-image-2.5-sunburst-2026-09-08`, `gpt-image-2.5-flare-2026-09-08`.
- Text to image: `POST https://api.openai.com/v1/images/generations`, JSON, bearer authentication. With references: `/v1/images/edits`, documented JSON field `images` with embedded data URLs (`images[].image_url`). No external image URLs, no file uploads for persistent provider storage, no redirects.
- App sizes: 1024×1024 (1:1), 1536×1024 (3:2), 1024×1536 (2:3), grouped as resolution `1K`. Explicit quality `low`, `medium` (default), `high`, `xhigh`, `max`. Switching models shows required format changes. Other API sizes, masks, streaming and transparency are not enabled in this integration.
- One PNG image per request (`output_format: png`, opaque background); series remain individual persistent jobs. The legacy `response_format` is not supported for GPT-Image and is not sent. JPEG/WebP/TIFF are independent local conversions.
- At most eight selected references (deliberately below the provider limit of 16). TIFF inputs are converted to PNG for transport only; the original stays unchanged. Limits: 20 MiB per data URL, 32 MiB of reference data URLs in total; response at most 32 MiB, decoded single image at most 20 MiB / 40 megapixels.
- The response contains embedded `data[].b64_json` image bytes. URL-only responses are not downloaded. The HTTP request ID is stored if present; it is not a retrievable job ID. No automatic retry after a timeout or unclear result, including 429/5xx.

A local test builds both real request types with a synthetic key, checks headers and redaction and that the key appears neither in URL nor JSON, and does not send them.

### OpenAI errors

[OpenAI error codes](https://developers.openai.com/api/docs/guides/error-codes) distinguish credit balance, project/organization spending limits, usage limits, rate limits and server overload. The [image guide](https://developers.openai.com/api/docs/guides/image-generation#handling-blocked-requests-and-other-errors) describes `image_generation_user_error` and `moderation_blocked`. FrameStudio evaluates these structured fields with an allowlist: specific code before general type, otherwise an understandable HTTP fallback. Only known parameter names such as `output_format`, `size` or `quality` are shown. No free-text error messages, parameter values, moderation details or unknown codes from provider responses reach the visible error text. Not every 429 is a short-term rate limit.

### OpenAI pricing

[Official pricing](https://developers.openai.com/api/docs/pricing), standard processing in USD, checked 2026-10-31, version `openai-images-standard-2026-10-31-v1`: text input 5 USD, image input 8 USD, image output 30 USD per million tokens. Batch discounts and caching are not assumed for these direct synchronous Images calls.

The output estimator follows the official GPT-Image-2.5 calculator from the image guide: quality grid long edge 16/24/48/64/96; short grid edge proportional to the image format with round-half-to-even; tokens = ceil(grid area × (2,000,000 + image pixels) / 4,000,000). Input text is estimated locally from UTF-8 bytes, and each reference image at a flat 8192 tokens. These input assumptions are shown explicitly in the UI and are not a guaranteed cap. Usage calculation uses `input_tokens_details.text_tokens/image_tokens`, `output_tokens` and consistent totals. A calculated amount is not a confirmed invoice.

Calculator evidence: official asset `https://developers.openai.com/_astro/GptImageTokenCalculator.react.CxseQDUo.js`, SHA-256 `00a23a754e5b3102f35f0823c5cbaa40c0bfbdc68f99eea6ce8f40067badf8b9`, checked 2026-10-31. Tests compare all three app sizes and five quality levels with the calculator values, e.g. 1024²: 196/439/1756/3122/7024 output tokens.

### OpenAI keys and live test

Keys for Google Gemini and OpenAI are stored, replaced and deleted separately in the settings. A ChatGPT login alone is not enough; you need an OpenAI API key for your own API account. Eligible Pro/Business subscriptions may provide API credits after explicitly linking the account; see the official [API credits guide](https://developers.openai.com/api/docs/guides/chatgpt-api-credits). FrameStudio never activates billing, account linking or top-ups.

Live test: the first explicitly approved photo request reached the Images edit API but was rejected with HTTP 429 / `credit_balance_exhausted`. Keychain access to the stored key worked. No image generation is confirmed yet; usage was missing and costs stay unknown.

## Anthropic Claude (text assistant only)

The official [vision documentation](https://platform.claude.com/docs/en/build-with-claude/vision) states that Claude does not generate or edit images. FrameStudio therefore offers no Claude image provider. The optional prompt assistant uses `claude-sonnet-5-5`, documented in the [model overview](https://platform.claude.com/docs/en/models/sonnet-5-5/overview): text input/output, 2 USD input and 10 USD output per million tokens in the standard tier. These prices are not directly comparable with image providers.

Contract: `POST https://api.anthropic.com/v1/messages`, bearer header, `anthropic-version: 2026-10-21`, `max_tokens: 4096`, `thinking: {type: between_tools}`, without tools, caching, image inputs or streaming. Sources: [Messages API](https://platform.claude.com/docs/en/api/messages/create), [authentication](https://platform.claude.com/docs/en/api/overview#authentication). The integration requires a workspace-scoped API key; account-wide keys without a workspace are not supported. Only the fixed system text and the description and change instruction visible in the dialog are sent; no implicit projects, styles or references.

Separate native key account `anthropic-api-key`. No stored keys in the UI, REST/MCP or exports. Fixed TLS endpoint, redirects off, 15 s connect / 120 s request timeout, response limit 1 MiB. Missing usage stays unknown; complete usage is calculated from the historical price snapshot. Refusals, truncated or empty answers are not offered as a finished suggestion. An unclear outcome is stored without automatic retry. Text jobs reserve the same project budget as image jobs; costs remain separable by kind.

Live test open: no paid Claude request was made during development. The local deterministic text assistant is clearly labeled as a test.

## Manual price refresh

The button loads only public standard rates from the sources above. Anthropic is checked via the official [Markdown pricing page](https://platform.claude.com/docs/en/about-claude/pricing.md) and [vision capability description](https://platform.claude.com/docs/en/build-with-claude/vision.md). Google and OpenAI use their HTML price tables. The OpenAI calculator is additionally checked against the documented SHA-256 but never executed. A changed calculator requires a verified app update.

Per source at most 2 MiB, 20 s request / 7 s connect timeout, no redirects and only six fixed HTTPS URLs. Parsers require unambiguous model rows, standard tier, USD/token units and plausible positive values. Version, source hash and check time are stored. Partial failures keep the last valid model price and its check date; the date of the failed attempt is stored separately. Unknown models are never added automatically.

The new catalog is stored atomically. Concurrent refreshes are locked per data store; no SQLite transaction stays open during network access. Price changes apply to new estimates and jobs. A GUI estimate that has become outdated causes an error before start, asking for a new check. Historical jobs and their usage calculations keep the embedded price basis. A refresh does not reconcile account discounts, invoices, credits, taxes or individual contract prices.
