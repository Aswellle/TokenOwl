# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

We take security seriously. If you discover a security vulnerability in TokenOwl,
please report it responsibly.

**Do NOT open a public GitHub issue for security vulnerabilities.**

Instead, please report vulnerabilities by opening a
[private security advisory](https://github.com/Aswellle/TokenOwl/security/advisories/new)
on GitHub.

Please include:

- A description of the vulnerability and its impact
- Steps to reproduce or a proof of concept
- Any relevant version or platform information

We aim to acknowledge reports within 72 hours and will keep you informed of
the progress towards a fix.

## Security Measures

- Credentials are stored in the OS keychain (Windows Credential Manager, macOS Keychain)
- API keys are never persisted in SQLite or logs
- Network requests enforce HTTPS-only endpoints
- Response size limits prevent memory exhaustion
- Path traversal protection on endpoint construction
- Content Security Policy restricts resource loading
- Telemetry and logging redact sensitive fields

## Scope

Security reports are welcome for:

- Authentication and credential handling
- Network request and response processing
- Data persistence and storage
- Dependency vulnerabilities
