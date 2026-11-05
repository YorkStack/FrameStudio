# Security policy

*Deutsch: Sicherheitslücken bitte vertraulich über „Report a vulnerability“ melden (siehe unten), nicht als öffentliches Issue.*

## Supported versions

| Version | Supported |
|---|---|
| 0.3.x | Yes |
| < 0.3 | No |

## Reporting a vulnerability

Please **do not open a public issue** for security problems.

Use GitHub's private reporting instead: open the repository's **Security** tab and choose **Report a vulnerability**. Include the FrameStudio version, operating system, steps to reproduce and the impact you expect. You should receive a first answer within a few days.

Never include real API keys, access tokens, private prompts or personal photos in a report. If a key was exposed, revoke it at the provider first.

## Scope

In scope: the FrameStudio desktop app, its REST and MCP interfaces, project import/export and the handling of provider keys and local access tokens.

Out of scope: the provider APIs themselves (Google, OpenAI, Anthropic), and attacks that already require full control of the local OS user account. FrameStudio deliberately trusts the local OS user; see the trust model in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Design documentation

- [Security design](docs/SECURITY-DESIGN.md) — series approvals, key access, public distribution
- [Architecture and security model](docs/ARCHITECTURE.md) — credential storage, REST/MCP boundaries, input handling
- [Keychain troubleshooting](docs/KEYCHAIN.md)
