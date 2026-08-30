## What this changes

<!-- The reasoning belongs here and in the commit messages, not only in the diff.
     Whoever reads this in six months — person or agent — has only the words. -->

## How it was verified

<!-- Not "tests pass". Which tests, and how you know they ran: this repository has a
     history of suites reporting hundreds of passes in a fraction of a second without
     compiling. Read the count and the clock. -->

- [ ] `cargo test -p arm7tdmi -p eapp-loader -p eapp-inspect`
- [ ] `cargo build --workspace` — the GUI is **not** covered by CI, so check it here
- [ ] If this touches a gate: I have watched it fail, not only pass

## Bypasses

<!-- If this adds a per-title workaround or a special case, give its retirement
     condition — what would have to become true for it to go away. Without one,
     temporary becomes permanent by default rather than by decision. -->

## Anything this deliberately does not do
