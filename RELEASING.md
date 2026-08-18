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
cargo test --release --workspace          # 220 at 0.5.0
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

### The build can lie about having happened

**Check the timestamps.** 0.4.0's first Linux build reported `Finished in 13s` and produced **0.3.0
binaries**. Nothing had been rebuilt.

The cause: the rustup toolchain on the Linux box ships `gcc-ld/ld.lld` as a shim that nixpkgs
patched to call `ld-wrapper.sh` *inside rustup's own nix store path*. That path was garbage-
collected, so the linker was pointing at a file that no longer existed — and cargo, unable to link,
served cached artifacts and reported success. `cargo test` failed loudly; `cargo build` did not.

Two defences, both now in the build script:

- **Linux builds use nix's own `cargo`/`rustc`**, which have no such shim. rustup is needed only for
  the Windows target, because nix's rustc does not carry the `windows-gnu` std.
- **`nixpkgs#rustup` stays in the nix shell** for the Windows build, which keeps the store path the
  shim references alive. Realising it again recreates the identical path, since it is the same
  derivation.
- **The script prints the binaries' mtimes at the end.** They must be today's. This is the check
  that caught it and it is the reason the step exists.

Delete `target/` when in doubt. A stale artifact is indistinguishable from a fresh one at a glance,
and that is the whole problem.

### Before publishing

Run the binaries. Not "they linked" — run them.

- [ ] `cargo test --release --workspace` on **every** platform built. The count is the check: **231**
      (2026-08-18). **Run it at the workspace, and read the count.** `cargo test -p eapp-loader`
      answers `0 passed; 0 failed … 0.00s` and calls it `ok`, because the crate's tests are in the
      lib target and that invocation reached a bin. A green line with a zero in it is not a pass,
      and this checklist is the only place that says so. *(This line read 183 until today, which is
      the same failure one level out: a count that is not re-measured stops being a check and
      becomes a number.)*
- [ ] Extract an archive somewhere else and run from there. `ipod-boot` finds `trace` beside
      itself, and that is what makes an archive work with no configuration.
- [ ] `ipod-emulator --check-update` exits 0 with no network.
- [ ] `ipod-emulator --check-images --flash=no --disk=no` says `UNREADABLE`.
- [ ] All **six** recipes compose: `for r in retail warm flsh rockbox flash-update from-idle`, each
      with `--print`. **`cold` is not one of them and must not be added back.** It was the
      prototype's NOR, it booted a firmware partition the retail ROM correctly rejects, and shipping
      it sent at least one person hours down a path that cannot work — `retail-boot.sh`'s header
      carries the measured difference. This list said `cold` until 2026-08-18, so the gate failed on
      a retired recipe, and the obvious way to make it pass again was to resurrect the recipe.
      **A checklist that fails for the right reason still teaches the wrong fix if it names the
      wrong thing.**
- [ ] macOS: `codesign -v "iPod 5G.app"`, and the bundle reports the right version and name —
      `plutil -p Contents/Info.plist`.
- [ ] Windows: run it. `wine` on the Linux box is enough to prove the recipes compose and the
      image validator answers. A binary that has only been linked is a binary nobody has run.
- [ ] `strings <binary> | grep -c /Users/` and `/home/` — both zero.
- [ ] **Every binary is dated today.** See above; this is not paranoia, it has happened.

### After

- [ ] `gh release view vX --json assets` — the archives are actually attached.
- [ ] `ipod-emulator --check-update` finds the new tag. GitHub's releases API caches for a minute;
      if it still reports the old one, wait rather than debug.
