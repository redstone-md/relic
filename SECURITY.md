# Security policy

## Supported versions

Relic is before 1.0. Only the latest release gets security fixes.

## Reporting a problem

Report it privately through
[GitHub security advisories](https://github.com/redstone-md/relic/security/advisories/new).
Please do not open a public issue for it.

Include the version, what an attacker can do and the steps to reproduce it.
You will get an answer within a week. Once a fix is released the advisory is
published and you are credited, unless you would rather not be.

## What counts

Relic is built on these promises; breaking any of them is a security problem:

- Nobody reaches the admin API without an administrator session.
- A player can read and change only their own account, skin and sessions.
- Passwords, invite codes and reset codes are stored only as hashes.
- The launcher installs an update only if it carries a valid signature from
  the key it was built with.
- The launcher writes only inside its own data folder, whatever paths the
  server sends.
- A file path sent to the daemon never escapes the folder it belongs to.

## What does not

The daemon's administrator decides which mods players run. A malicious
administrator, or a malicious mod they chose to distribute, is outside what
Relic can defend against: players trust the server they join.
