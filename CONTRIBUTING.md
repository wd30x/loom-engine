# Contributing to loom-engine

Thanks for your interest! loom-engine is a small, opinionated project. These guidelines explain what fits and how to get a change merged.

Issues and pull requests are welcome in English or Chinese. The project documentation (`CLAUDE.md`, `docs/`) is written in Chinese; code, comments and commit messages are in English.

## Before you start

- **Features are driven by real needs.** The engine grows only when one of its games (`games/souls`, `games/rogue`) needs something. Please open an issue before starting a new feature or a large change, so we can agree on whether and how it fits.
- Bug reports, fixes, documentation improvements and small cleanups can go straight to a pull request.
- Read [CLAUDE.md](CLAUDE.md) for the project rules and [docs/architecture.md](docs/architecture.md) for the conventions (coordinate system, units, colour space, file formats). Don't invent new conventions; extend the architecture document instead.

## Setup

- Rust: the toolchain is pinned in `rust-toolchain.toml`; `rustup` picks it up automatically.
- [Git LFS](https://git-lfs.com/): binary assets under `assets/` are stored in LFS. Run `git lfs install` before cloning.
- [PowerShell 7](https://github.com/PowerShell/PowerShell) (`pwsh`) for the scripts in `scripts/`.
- The engine is developed and tested on Windows. Other platforms may work but are not tested yet; reports are welcome.

```powershell
pwsh scripts/verify.ps1                                           # fmt + clippy (-D warnings) + tests
cargo loom run --headless --frames 60 --screenshot out/shot.png   # quick headless run
pwsh scripts/smoke.ps1                                            # GPU and window paths, needs a desktop session
```

## AI-first rules

These are hard rules. A change that breaks one of them will not be merged.

1. **Everything is text.** Scenes, config, data and input recordings use RON or JSON, never binary formats.
2. **Headless.** Every feature must work with `--headless`, without creating a window or depending on winit.
3. **Deterministic.** Simulation advances only in fixed-step ticks. Simulation code must not read wall-clock time, depend on `HashMap` iteration order, or use unseeded randomness. The same input must produce byte-identical output.
4. **Observable.** A new feature must be checkable through a screenshot, a JSON state dump or a log. If it can't be observed, it isn't done.
5. **stdout is for machines, stderr is for humans.** `loom` writes only JSON to stdout; all logs go through the `log` macros to stderr.

If you change anything visual, take a headless screenshot and look at it before opening the pull request.

## Code

- Respect the crate boundaries listed in [CLAUDE.md](CLAUDE.md). Engine crates must not contain game-specific logic; games must not depend on the renderer, the platform layer or each other.
- Library crates define errors with `thiserror`; only `loom_engine` uses `anyhow`.
- No `unwrap()` in engine code. Where failure is truly impossible, use `expect("why it cannot fail")`.
- No `unsafe` (denied at the workspace level). If you believe you need it, raise it in an issue first.
- Comments and identifiers are in English.

## Tests

- Unit tests live next to the code (`#[cfg(test)] mod tests` or a sibling `*_tests.rs`); `tests/` holds cross-crate integration tests only.
- Name tests with a full sentence describing the guaranteed behaviour, e.g. `long_frame_is_capped_and_excess_dropped`. Test names must not contain each other, because `cargo test` filters by substring.
- Timing-related tests must pass 10 runs in a row. Don't hide flakiness by raising timeouts.
- Some CLI tests compare against files in `crates/loom_engine/tests/cli/expected/`. If your change alters that output on purpose, update the expected files in the same commit and explain why in the pull request.

## Assets

- Contribute only assets you made yourself or that are under a license compatible with this project, and state their origin in the pull request.
- Prefer assets that can be rebuilt from text: an Aseprite Lua script or a Blender Python script, plus the exported files. Each asset directory has a README describing how its files are produced; update it with your change.
- When porting code from another project (for example Godot), keep its license notice in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Don't copy code from Godot's `thirdparty/` directory, whose licenses vary.

## Commits and pull requests

- `pwsh scripts/verify.ps1` must pass before every commit.
- Keep one logical change per commit. Commit messages are in English and use the form `<crate or scope>: <what changed>`, e.g. `loom_render: add offscreen target and readback`.
- In the pull request, describe what changed and how you verified it: commands you ran, their output, and screenshots for visual changes.
- Changes made with the help of AI agents are welcome, under the same rules: you are responsible for reviewing the result and for the verification above.

## License

loom-engine is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option. Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
