# Changelog

Notable changes to Relic. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.0.1] - 2026-11-12

The first public release.

### Added

- Daemon that installs and supervises several Minecraft servers: Fabric,
  Quilt, Forge and NeoForge, Minecraft 1.12.2 and newer, with the matching
  Java runtime downloaded automatically.
- Admin panel built into the daemon: dashboard, live console, file manager
  with a trash, game rules, worlds, players, backups, news and settings, in
  English and Russian, with light and dark themes.
- Modpack management: uploads, `.mrpack` import, search of Modrinth and
  CurseForge, a modpack catalogue, optional mods and automatic detection of
  the side each mod belongs to.
- Player accounts with invites, password reset codes, skins and capes,
  served to the game through authlib-injector.
- Launcher for Windows, Linux and macOS that installs the game, syncs the
  modpack by hash, starts the game and updates itself from signed releases.
- Docker image, systemd unit and release builds signed with minisign.

[0.0.1]: https://github.com/redstone-md/relic/releases/tag/v0.0.1
