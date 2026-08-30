# Contributing

Three people work on this, each with their own machine, their own agent tooling, and their
own hours. This file is what keeps that from turning into merge conflicts and duplicated
work. It is short on ceremony deliberately.

## Where things live

| | |
|---|---|
| `siggifly/ipod-emulator` (GitHub, public) | the emulator. Where pull requests happen. |
| `siggifly/clickwheel` (GitHub, private) | the pipeline: decryption, lifting, recompilation, the research trees. |
| `code.veldi.io` (Forgejo) | **runs the gates.** Not a place you need an account. |

**You do not need Forgejo access.** GitHub is the collaboration surface — issues, pull
requests, review, discussion. Forgejo is where CI executes, because GitHub Actions is
disabled at the account level and cannot run. Results come back to you in Discord
`#github` and in the pull request; you never have to visit it.

That split is not a preference, it is a constraint being worked around. It may change.

## Branches

```
master          last released. Protected. Moves only at a release.
dev             the real trunk. Always green, may be half-finished. Default branch.
area/topic      your work in progress — drm/…  recomp/…  gui/…  research/…
release/N.N     cut from dev when a release is feature-done.
```

Branch from `dev`, pull-request into `dev`. The area prefix is not bureaucracy: it is what
lets notifications route and lets each of us filter to our own lane without reading
everything.

Nothing goes to `master` except a release. A force-push to it is refused by the server,
which has been tested rather than assumed.

## The gates

Every push to `dev` and every pull request runs:

| gate | refuses |
|---|---|
| `build-check` | `arm7tdmi`, `eapp-loader`, `eapp-inspect` failing to build or test. Reads the test **count and the clock** — a suite that "passes" in under five seconds did not compile. Floor is 250 against a measured 294. |
| `no-secrets` | anything under `resources/` entering git, and credential-shaped strings. |

**`ipod-gui` is not covered.** It needs GL, X11 and Wayland headers, and the runner has no
root to install them. A green tick does **not** mean the window builds — check that
locally before claiming it does. This hole is real and known; do not let it surprise you.

Before pushing, the cheap local equivalent:

```sh
cargo test -p arm7tdmi -p eapp-loader -p eapp-inspect
cargo build --workspace          # this one does cover the GUI
```

## Working with an agent

All three of us drive this with coding agents on subscription plans — Claude Code, Codex,
whatever you prefer. A few things make that work much better here, and they are cheap.

**Read `AGENTS.md` first, and point your agent at it.** It is 200 lines and it is the
accumulated scar tissue of this project: the accuracy rule, "the instruments lie", what
must never enter git, what to do before claiming something works. `CLAUDE.md` is a
one-line pointer at the same file, so one document serves every tool.

**The rules that matter most, because they are the ones this project keeps re-learning:**

- **Prove a gate can fail before trusting it.** A green light nobody has watched go red is
  not a gate. Nine published conclusions here have been lost to instruments that failed
  silently.
- **Read the count and the clock.** `cargo test` has reported hundreds of passes in a
  fraction of a second without compiling anything. Both numbers, every time.
- **A measurement without its recipe is not a measurement.** State the ROM, the firmware,
  the disk image and the instruction budget, or the number cannot be reproduced or
  compared.
- **Retract in place.** When something recorded here turns out to be wrong, correct the
  document rather than adding a newer one that disagrees. A wrong comment is worse than no
  comment, because the next person plans around it.

**Give the agent the research, not just the code.** `research/` in this repository and
`research/` in `clickwheel` hold most of the hard-won facts. Grepping them first avoids
re-deriving something that took a week the first time. That has happened more than once.

## Pull requests

Small enough to review, with the reasoning in the commit message rather than only in the
diff. Commit messages here are prose and explain *why*; matching that style is genuinely
useful to whoever reads it in six months, agent or human.

If a change adds a per-title workaround or a bypass, give it a **retirement condition** —
what would have to become true for it to go away. Without one, temporary fixes become
permanent by default rather than by decision.

## Hardware

Real devices are the scarcest resource here. If you have one, `research/` should record
what it is and what has been dumped from it, and every dump should carry its provenance:
which device, which firmware build, which address range. A dump nobody can trace is a
number nobody can re-derive.

Open questions that need hardware live in Discord `#hardware`, so that when a device is
open on a desk it can answer several at once.

## Getting the large files

`resources/` is not in git and never will be — it holds Apple's firmware, ROM dumps,
multi-gigabyte disk images, and real people's identifiers. Ask in Discord for the subset
you need; most work needs a few hundred megabytes rather than the whole tree.
