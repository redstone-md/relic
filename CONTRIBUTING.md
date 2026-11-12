# Contributing to Relic

Thanks for taking the time. Bug reports, fixes and features are all welcome.
For anything larger than a fix, open an issue first so the direction is agreed
before you write the code.

## Setup

You need Rust 1.88 or newer, Node 22 and pnpm 9. On Linux the launcher needs
a few development packages; the exact `apt-get` line is in
[`.github/workflows/ci.yml`](.github/workflows/ci.yml).

```sh
# The daemon embeds the built panel, so this comes first.
cd web && pnpm install --frozen-lockfile && pnpm build && cd ..

cargo run -p admin-core          # daemon on 0.0.0.0:8080, data in ./data
cargo run -p launcher-client     # launcher
```

While working on the panel, keep the daemon running and start `pnpm dev` in
`web/`. Set `RELIC_LAUNCHER_DIR` to give the launcher a throwaway folder
instead of your real one.

## Before you open a pull request

CI runs these on Linux, Windows and macOS, with warnings treated as errors:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
cargo test --workspace
cd web && pnpm build
```

Tests marked `#[ignore = "network"]` download real game files. They are not
part of CI; run them by hand when you touch installation code. Running them
means you accept the [Minecraft EULA](https://aka.ms/MinecraftEULA).

## Conventions

- Commits follow [Conventional Commits](https://www.conventionalcommits.org):
  `feat(launcher): ...`, `fix(daemon): ...`, `docs: ...`. One logical change
  per commit.
- `unsafe` is forbidden across the workspace.
- Every string a person reads exists in English and Russian: the panel keeps
  its translations in `web/src/locales`, the launcher in
  `crates/launcher-client/locales`.
- Interface copy is plain and in sentence case. The panel and the launcher
  use square corners throughout.
- A change to the protocol between daemon and launcher goes into
  `relic-proto` and stays compatible with launchers already in the field.

## Reporting security problems

Not in a public issue, please. See [SECURITY.md](SECURITY.md).

## License

By contributing you agree that your work is released under the
[MIT license](LICENSE).
