# Security policy

Report a vulnerability privately to the repository maintainer using GitHub private vulnerability reporting once it is enabled. Do not post tokens, local paths, or exploit details in a public issue.

The frontend has no generic shell, filesystem, HTTP or opener plugin permissions. Native commands accept typed settings and repository IDs. The IPC boundary is local, the content security policy disables remote code and frames, and user-controlled HTML is escaped. Public repository metadata never supplies executable command strings.

Review terminal commands before confirming. Package managers and Git contact upstream services; repository popularity is not proof of security. A compromised local OS, writable executables on PATH, or a malicious installed shell profile is outside this application's trust boundary. Keep your toolchain and platform runtime up to date.

The workflow periodically checks resolved Rust dependencies against the RustSec advisory database. A successful advisory scan is not a full security audit. Configure code signing and complete desktop acceptance before public production releases.
