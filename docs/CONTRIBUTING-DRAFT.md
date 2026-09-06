# Contributing

**Draft.** Written 2026-09-06, when this stopped being a one-person project. Everything in it is
checkable against the repository as it stands; where something is a proposal rather than current
practice it says so.

The [README's Contributing section](../README.md#contributing) is still the entry point — say hello
first, because most of the work needs Apple's firmware, ROM dumps and multi-gigabyte disk images
that cannot be published, and a pull request arriving cold has none of that context. The Discord
invite lives in the README and **only** in the README; if you find it copied anywhere else, that
copy is the one that will go stale.

This file is about what happens after that: how work is claimed, how it lands, and the handful of
rules that a contributor's agent will otherwise violate confidently.

---

## Why this file exists at all

The working contract for agents in this repository is `AGENTS.md`, and **`AGENTS.md` is deliberately
not committed.** It is written for whoever works on the program, not for whoever reads it, and this
is a public repository whose documentation should be about the program.

That was fine while one person had it on disk. It is not fine now: a contributor's agent that has
never read it will re-derive things that are already written down, tidy away a wrong answer that was
kept on purpose, and report an absence it could not have observed. So the load-bearing half of it is
restated below, in this repository's own words, for a reader who will never see the original.

---

## Claiming work

**Assign yourself to the issue on GitHub. That is the claim.**

Not a comment saying you are taking it. An assignee is a structured field: it shows in
`gh issue list --json assignees`, it shows in the issue's header without scrolling, and an agent
that checks before starting will see it. A comment is prose in a thread that may be forty comments
long, and an agent will read past it. We already lost work this way — two people built the same
thing in one week.

- **Check before you start**: `gh issue list --state open --json number,title,assignees`
- **Unassign yourself if you stop.** An issue assigned to somebody who has moved on is worse than an
  unassigned one, because it reads as covered.
- **If an issue is assigned and you want it, ask in the thread.** Do not start in parallel "just to
  compare". Two branches that both work is a merge problem, not a bonus.

### One issue = one lane

Every issue that is ready to be picked up carries a **file scope**: the files the taker owns for the
life of the issue. Nobody else edits those files while it is open. That is the whole mechanism for
keeping parallel agents out of each other's way, and it is the field worth arguing about before the
work starts rather than after.

Two things make the scope real rather than decorative:

- **`tools/ipod-machine/src/lib.rs` is the collision surface.** It is over eleven thousand lines and
  nearly every subsystem has a hunk in it. An issue that says "owns `lib.rs`" owns the repository.
  Say *which hunks* — the region table, the bus hook, the three functions at lines N..M — and keep to
  them.
- **`docs/GUI.md` is the design of the window and is edited section by section.** Two issues can
  legitimately touch it at once if they name different `§`s. One that says "owns `GUI.md`" does not.

If you find you need a file outside your scope, that is not a reason to take it. It is a second
issue, with its own scope, and usually a better one than widening the first.

---

## Branches and pull requests

| | |
|---|---|
| default branch | **`dev`** — this is what pull requests target |
| also present | `master` |
| **not** present | there is **no `main` branch**, despite what `NEXT.md` R11 says |

**Work on a fork. Open a pull request from your fork's branch to `dev`.** Do not push a branch to
the shared repository, and do not push to `dev` or `master` at all. `NEXT.md` R11 says direct commits
to `main` are refused by a hook; that sentence names a branch that does not exist and a hook that is
not installed in a fresh clone (`.git/hooks/` holds only the `.sample` files, and `core.hooksPath` is
unset). Treat the rule as real and the mechanism as absent — nothing will stop you, so do not.

One branch per topic, named for the topic. One issue per branch.

### Nobody's fork PR gets automated checks. Run the gates yourself.

This is the most surprising fact about this repository and it costs people real time:

- `.github/workflows/ci.yml` builds Linux, macOS and Windows, and **has never executed once.**
  `gh api repos/siggifly/ipod-emulator/actions/runs` answers `total_count: 0`, and a manual dispatch
  answers `HTTP 422: Actions has been disabled for this user`. That is account-level; the repository
  API still cheerfully reports `enabled: true`.
- The gates that actually run live in `.forgejo/workflows/ci.yml` and run on a **different forge**.
  They fire on pushes and pull requests to `dev` and `master` there. A pull request opened on GitHub
  does not reach them.
- PR #3 merged with **no checks reported**. That is not an oversight in that PR; it is the state of
  every pull request here.

So run them locally and say in the PR that you did:

```sh
cargo build --workspace                                       # the GUI is not in any CI leg
cargo test -p arm7tdmi -p ipod-machine -p eapp-inspect -p docgen
cargo clippy --workspace --all-targets
```

Warnings are errors in practice — the tree is at zero and stays there.

**Read the count and the clock.** This suite has reported "275 passed" in 0.26 seconds without
compiling anything. The Forgejo gate refuses a run that finishes in under five seconds or reports
fewer than 420 passes for exactly that reason. A pass that arrives impossibly fast is not a pass.

The pull-request template asks how you verified, and it does not accept "tests pass". Which tests,
and how you know they ran.

### Who merges, and who rebases

**The operator merges.** Not because review is ceremonial — because merging is where a change meets
the disk images, the ROM dumps and the recipes that no CI leg can touch, and that check happens on
one machine.

**Whoever caused the drift rebases.** If your branch conflicts because you sat on it, that is yours.
If the base moved three hundred commits under you because a long-unpushed backlog landed, **that is
the project's doing and the project rebases it, or walks you through it.** That has happened once
already: 206 commits went to `master` in one push and flipped an open pull request from `CLEAN` to
`CONFLICTING` overnight. The conflict was textual — nothing had been reimplemented by anyone else —
and it was not the contributor's to absorb silently.

If your base moves under you, say so in the thread rather than force-pushing a guess. A conflict *is*
the work; it is not a reason to abandon a branch.

---

## The rules that will otherwise be violated confidently

These four are the ones an agent gets wrong by default, because each of them contradicts a habit
that is correct almost everywhere else.

### 1. If a stock operating system disagrees with this emulator, the emulator is wrong

Fix the machine. Do not branch per operating system, do not special-case a build, do not patch the
OS to agree with us.

Rockbox and iPodLinux are **oracles** — they are how we find out we are wrong, and that only works if
we run what upstream ships. Running a second stack has already found four device models that Apple's
own firmware could never have exercised, because each of them was shaped around Apple's driver rather
than around the part. The stack that shaped the model is the stack that cannot fail against it.

The corollary contributors hit first: when our behaviour and an upstream tool's behaviour differ, the
answer is *usually* ours, and when it genuinely is theirs the fix goes **upstream**, not into a local
workaround. There is a live example in `ROADMAP.md` §M4: a real iPod's own drive presents MBR
partition type `0x0C`, `ipodloader2` handles only `0x0B`, and our image writer already emits `0x0B`.
"Fixing" our writer to match the loader would make iPodLinux boot and would quietly encode a false
belief about the hardware into every disk we make. **Do not.**

### 2. The instruments lie

Eight of them have reported an absence they could not have observed. That has cost more time than
every real bug in the emulator combined.

**Before believing a zero, run the control that makes the instrument produce a non-zero.**

Shapes that have actually happened here:

- An input script anchored in **executed instructions** on a machine that spends its budget halted: a
  3 G budget executed 495 M and fired **0 of 12** steps, which read as "the wheel is not being
  listened to". Anchor injected input in **simulated time** (`@24s:touch`, never `@2200M:touch`) and
  read the `script: N of M steps fired` line before believing anything about input.
- A fix that changes nothing because a fast path swallowed it. If a change to the machine leaves the
  instruction count *exactly* identical, the change did not take effect.
- A test that passes with the fix reverted, because the fixture was already in the state the fix
  produces. Prove the test fails without the fix, or it tests nothing.
- A comparison that let each arm resolve its own paths, so it compared two machines as well as two
  builds. Pin `FLASH=`, `DISK=`, `BUDGET=` and `WORKDISK=` explicitly, in **both** arms.
- A capped log printing its cap as if it were a count. Every capped instrument now prints its
  uncapped census as the headline and says `SAMPLE, NOT A CENSUS` when the rows below are truncated —
  read the headline, not the rows.

`NEXT.md`'s final section lists every instrument with a note on how each one lies. That section is
not decoration, and it is the first thing to read before you quote a number from a run.

**Numbers carry the recipe that produced them.** A figure without the command that made it cannot be
rechecked and will go stale silently. Every measurement you publish — in a PR body, in `research/`,
in a comment — gets its command written beside it.

### 3. A bypass needs a retirement condition

Where something is faked to get past a wall, it goes in
[`research/04-bypass-ledger.md`](../research/04-bypass-ledger.md) **with a retirement condition** —
the observation that would let it be deleted.

**A bypass with no retirement condition is a lie with a comment on it.** Temporary becomes permanent
by default rather than by decision, and six months later nobody can tell which of the numbers in
`research/` were measured through it. The pull-request template has a section for this; it is not
boilerplate.

The same discipline applies to a `#[allow(dead_code)]`, a hardcoded constant that "works", and a
special case for one title: say what would make it go away.

### 4. Retracted answers stay where they are

`research/` is the larger half of this project and is licensed separately (CC BY-SA 4.0) so people
can quote it. It keeps what was believed and why it was wrong, **in place**.

When a conclusion is retracted, the retraction sits next to the claim. **Do not tidy away a wrong
answer** — the wrong answer is how the next person avoids it. `research/03` alone carries five
retractions, and at least twice a question was worked for days as an open mystery when its answer was
already filed as a retraction somewhere else.

Two working consequences:

- **A document that grows by addendum puts its live answer at the bottom** and its superseded ones
  above, and nothing on the page says so. Start at [`research/FINDINGS.md`](../research/FINDINGS.md),
  which is *generated* from the corpus by `cargo run -p docgen` and kept honest by a test that fails
  on drift. It answers the one question grep cannot: which answer is current.
- **`grep` is still how you search.** The index says where a document stands; it does not contain the
  corpus, and no summary of thirty-five thousand lines ever will.

Everywhere else in this repository the rule is the opposite — clean-first, no compatibility shims, no
"previously…" prose, delete the code you replaced. `research/` is the one exception, and it is the
one an agent will violate first.

---

## Two more that are not negotiable

### Nothing under `resources/` ever enters git

`resources/` holds Apple's firmware, ROM dumps, shipping game binaries, multi-gigabyte disk images,
and **real people's names, Apple IDs, serial numbers and FireWire GUIDs**. It is gitignored in two
forms — `resources` and `resources/` — deliberately: the bare form matches the *symlink* that agents
create in worktrees to run the recipes, and a symlink is not a directory. That symlink was once one
`git add -A` from being committed.

- Never `git add` anything under `resources/`, and never the symlink.
- Never quote a real serial, GUID, name or Apple ID from it into a tracked file, a commit message, a
  test fixture, a log line or a doc. Generated identities are generated; real ones stay in
  `resources/`.
- A purchased title's `iTunesMetaData` and the `.sinf` beside its executable are the **purchaser's**.
  Treat them like a serial number.

The Forgejo `no-secrets` job checks `git ls-files | grep '^resources/'` — but per the CI section
above, **it does not run on a GitHub fork's pull request.** Check it yourself before you push:

```sh
git ls-files | grep '^resources/' && echo "STOP"
```

Once an identifier is pushed to a public repository it stays reachable. This is the one rule where a
mistake cannot be undone.

### Say what you actually did

The person reading your pull request cannot see your tool output. What you say happened is all they
have, which makes an unverified claim indistinguishable from a lie.

- **Ran** means you ran it, in this session, and read the output.
- **Fixed** means you made the change *and* observed the behaviour change. "I deleted the old screen,
  which fixes the loop" is a hypothesis; if the code that caused the loop is still there, the claim is
  false — that has happened here.
- If tests fail, say so and paste the failure. If a step was skipped, say which.
- **Verify what you ship, not what is on disk.** A gate you cannot make fail is a gate you have not
  tested.
- Correct a wrong claim plainly, once, and move on. Nobody is keeping a tally.

And: **fix errors you find even when they are not yours**, and even when they are outside the task.
Say what you fixed.

---

## Where things live

| | |
|---|---|
| `tools/arm7tdmi` | the CPU — no dependencies, no `unsafe` |
| `tools/ipod-machine` | the machine: memory map, peripherals, ATA, flash, the co-processor. Ships `ipod-boot`, `trace`, `ipod-film`. Also the model — `settings.rs`, `compose.rs`, `identity.rs`, `nor.rs` |
| `tools/ipod-gui` | the window |
| `tools/ipg-player` | one `.ipg` title in a window |
| `tools/eapp-inspect` | reading Apple's binaries |
| `tools/docgen` | generates `research/FINDINGS.md` and fails a test when it drifts |
| `tools/ghidra` | the headless decompiler path |

**The model lives in `ipod-machine`, not in the window.** `settings.rs`, `compose.rs` and
`identity.rs` know nothing about any UI toolkit and must stay that way — that is what makes the
window replaceable. `tools/ipod-gui/src/main.rs` is the only file that touches the toolkit; every
other file in that crate is toolkit-free. Keep it so. A UI toolkit inside the model is a mistake this
project has already made once and undone.

`docs/GUI.md` is the *design* of the window. It is written before the window, and it is the thing to
correct when the window is wrong.

## Which document answers which question

| | |
|---|---|
| [`ROADMAP.md`](../ROADMAP.md) | what is intended, in what order, and what would settle each milestone |
| [`KNOWN-BUGS.md`](../KNOWN-BUGS.md) | what is wrong |
| [`NEXT.md`](../NEXT.md) | what is being worked on now — and the instrument table |
| [`research/04-bypass-ledger.md`](../research/04-bypass-ledger.md) | what is faked, with a retirement condition for each |
| [`research/FINDINGS.md`](../research/FINDINGS.md) | which answer in `research/` is current |
| [`CHANGELOG.md`](../CHANGELOG.md) | what changed, release by release |
| [`docs/DEVELOPING.md`](DEVELOPING.md) | building it, the recipes, installing other operating systems |
| [`HARDWARE.md`](../HARDWARE.md) | what a real iPod could answer, if you own one |

**If you own a click-wheel iPod, that is the most useful thing you can offer** — more useful than
code, and usually an afternoon with no soldering.

---

## No calendars, no deadlines, no version-number promises

Milestones are artifacts that exist. A milestone is done when the thing it names can be pointed at,
not when a date arrives.
