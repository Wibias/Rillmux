# Security Policy

## Supported versions

Security fixes go into the latest GitHub Release of Rillmux. Older tags are not patched.

| Version | Supported |
| ------- | --------- |
| Latest GitHub Release | Yes |
| Earlier releases | No |

## Reporting a vulnerability

**Do not file a public GitHub issue, discussion, or pull request for a security bug.**

Report it privately with GitHub’s [private vulnerability reporting](https://github.com/Wibias/Rillmux/security/advisories/new) form (Security → Advisories → Report a vulnerability).

Include as much of this as you can:

- What is affected (auth/tokens, updater, installer, local files, etc.)
- Version or commit, and OS
- Steps to reproduce, and impact if someone exploited it

There is no bug bounty. We will open a draft advisory, fix on a private or coordinated timeline, and publish when a release is out.

In scope: anything that lets someone steal Twitch tokens, run code, tamper with the updater, or read another user’s data on this machine. Out of scope: SmartScreen warnings on unsigned installers, and issues that only apply to a local unsigned build.

Dependency advisories in npm or Cargo that are already public can go through Dependabot or a normal issue.
