# Releasing

Written because a release shipped binaries that called themselves `0.1.0` while the workspace said
`0.2.0`. The version was in a second place nobody was looking at. Everything below exists because
something like that happened, not because it seemed thorough.

## The rules

**One version, in `Cargo.toml` at the root.** `[workspace.package] version` — the four crates
inherit it with `version.workspace = true`. If you find yourself typing a version number anywhere
else, that is the bug; make the other place read this one.

**A published tag is never moved.** `v0.1.0` was moved once, hours after it was cut. Nobody had
downloaded it, so nothing broke — but had anyone been running the first build, their copy would
have compared its own version against the tag, found them equal, and reported itself current while
being four commits behind. Anything published gets a new number.

**The tag and the binaries must come from the same commit.** `v0.1.0` drifted four commits behind
the code its own archives were built from, which makes the tag a lie about what people downloaded.
Tag first, then build from the tag.

## What has to change, and what reads it

| | |
|---|---|
| `Cargo.toml` → `[workspace.package] version` | **the only edit.** Everything else derives |
| `ipod-emulator --check-update` | compares `CARGO_PKG_VERSION` against the latest release tag — this is why the tag and the crate version must agree |
| `iPod 5G.app/Contents/Info.plist` | `make-app.sh` reads `Cargo.toml`. It used to hold a literal, and that literal went stale |
| `CHANGELOG.md` | hand-written. Lead with what someone will notice, not what was refactored |

**Nothing else should contain a version.** Check it:

```sh
git grep -nE '"0\.[0-9]+\.[0-9]+"' -- ':!Cargo.lock' ':!CHANGELOG.md' ':!RELEASING.md' \
  ':!research' ':!NEXT.md' | grep -v eframe
```

Exactly two lines are expected, and both are fine:

```
Cargo.toml:24:version = "0.3.0"                              <- the one true version
tools/ipod-gui/src/update.rs:219:  parse_version("0.1.0")    <- a test fixture, not a version
```

Anything else is about to disagree with reality. This is how the `Info.plist` literal would have
been caught before it shipped instead of after.

There were also four **stale per-crate `Cargo.lock` files**, left over from before the workspace
existed and still claiming `0.1.0`. A workspace has one lockfile, at its root. They are deleted.

## The steps

```sh
# 1. Bump, changelog, commit, push.
$EDITOR Cargo.toml CHANGELOG.md
cargo test --release --workspace          # 183 at 0.3.0
git commit -am "0.X.0 — …" && git push

# 2. Tag the commit the binaries will be built from.
git tag v0.X.0 && git push origin v0.X.0

# 3. Build all four. See below — CI cannot do this yet.
# 4. Publish, with the archives attached.
gh release create v0.X.0 dist/* --title "iPod 5G emulator v0.X.0" --notes-file notes.md --verify-tag
```

### Building the four

`.github/workflows/release.yml` does all of this on a tag and is the reference. Until it can run,
it is done by hand — macOS locally, Linux and Windows on an x86-64 Linux box.

**Always set `RUSTFLAGS` to remap paths.** Without it every binary carries the absolute path of the
machine that built it — 395 occurrences of a home directory in one binary, before this was noticed.

```sh
export RUSTFLAGS="--remap-path-prefix=$HOME=~ --remap-path-prefix=$PWD=."
cargo build --release --target aarch64-apple-darwin  --workspace --bins
cargo build --release --target x86_64-apple-darwin   --workspace --bins
```

On the Linux box, via `nix shell` so nothing is installed permanently:

```sh
nix shell nixpkgs#gcc nixpkgs#pkgsCross.mingwW64.buildPackages.gcc nixpkgs#pkg-config \
          nixpkgs#libGL nixpkgs#xorg.libX11 nixpkgs#xorg.libXcursor nixpkgs#xorg.libXrandr \
          nixpkgs#xorg.libXi nixpkgs#libxkbcommon nixpkgs#wayland -c sh -c '…'
```

Three things that cost time there, all of which will cost it again:

- **A cross toolchain does not remove the need for a host `cc`.** Build scripts and proc-macros
  compile for the host and fail first.
- **`rustc` links `-l:libpthread.a` for `x86_64-pc-windows-gnu`.** A distribution that keeps
  mingw's winpthreads in its own place needs `-L native=…` pointing at it, or everything compiles
  and nothing links.
- **An environment that rebuilds `PATH` drops a toolchain that is not part of it.** Repairing that
  with an outer-shell `PATH=` expansion replaces the entries it was meant to preserve.

### Before publishing

Run the binaries. Not "they linked" — run them.

- [ ] `cargo test --release --workspace` on **every** platform built. The count is the check: 183.
- [ ] Extract an archive somewhere else and run from there. `ipod-boot` finds `trace` beside
      itself, and that is what makes an archive work with no configuration.
- [ ] `ipod-emulator --check-update` exits 0 with no network.
- [ ] `ipod-emulator --check-images --flash=no --disk=no` says `UNREADABLE`.
- [ ] All seven recipes compose: `for r in retail cold warm flsh rockbox flash-update from-idle`.
- [ ] macOS: `codesign -v "iPod 5G.app"`, and the bundle reports the right version and name —
      `plutil -p Contents/Info.plist`.
- [ ] Windows: run it. `wine` on the Linux box is enough to prove the recipes compose and the
      image validator answers. A binary that has only been linked is a binary nobody has run.
- [ ] `strings <binary> | grep -c /Users/` and `/home/` — both zero.

### After

- [ ] `gh release view vX --json assets` — the archives are actually attached.
- [ ] `ipod-emulator --check-update` finds the new tag. GitHub's releases API caches for a minute;
      if it still reports the old one, wait rather than debug.
