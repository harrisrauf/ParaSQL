# Security Policy

## Supported versions

| Version | Supported |
| ------- | --------- |
| 0.2.x   | ✅        |

## Reporting a vulnerability

Please **do not** open a public issue for security problems. Instead, use
GitHub's private vulnerability reporting:

1. Go to the repository's **Security** tab.
2. Click **Report a vulnerability**.
3. Describe the issue, reproduction steps, and affected version.

You can expect an acknowledgement within a few days. Please give us a
reasonable window to ship a fix before any public disclosure.

## Security model

ParaSQL is a local desktop application:

- All data stays on your machine. There are no network calls, accounts, or
  telemetry.
- The frontend runs under a strict Content-Security-Policy
  (`default-src 'self'`, no `unsafe-eval`, `object-src 'none'`).
- Tauri capabilities are scoped to the minimum required: window close/destroy
  and the dialog plugin's `open`, `save`, and `message` commands.
- File writes are atomic (temp file + rename) to prevent partial or corrupted
  files on failure.
- SQL executed through the app is restricted to read-only statements
  (`SELECT`/`WITH`) except for explicit editor operations.

If you find a path that violates any of the above, we want to know.
