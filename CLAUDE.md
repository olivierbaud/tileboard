# tileboard

A modular dashboard: apps describe their own data through two read-only endpoints (`/widgets`), and you build tiles from any mix of that data, then drag and resize them in edit mode. **On resume, read `docs/handover.md` first**, then `docs/plan.md`. **Then read `.private/NOTES.md` if it exists** (git-ignored: owner's paths, deploy target, app addresses, wrap-up duties).

## This repo is PUBLIC (MIT)
- **Nothing personal is ever committed:** no IPs, hostnames, Tailscale names, tokens, real app addresses, real layouts, real data, local paths or emails. Those live in `.private/` or a git-ignored config file. Fixtures, examples and screenshots use made-up data only (the demo app).
- No employer, customer or site names. No security / access-control integrations in this repo.
- Before every commit: `git diff --cached` and look for anything that wouldn't be safe on the public internet. GitHub secret scanning + push protection are on; they are a backstop, not the check.
- Commits use the GitHub noreply email (repo-local git config). Don't change it.

## Owner's choices (keep unless Olivier changes them)
- **Unlimited tiles, any mix of data:** "if you want just a widget with just 1 data, and another widget with another data, from the same app, then another with 2 datas of the same app again, it should be possible."
- **Apps give data points, the dashboard builds tiles** (widget contract v2, `docs/plan.md`). Apps never decide what a tile looks like.
- **Edit mode:** "when you click an edit button, everything is resizable and drag and drop, so you can change the positions." Done saves and locks.
- **Public repo, no private data, MIT licence.** Public from the first commit.
- **Rust server + plain JavaScript frontend, no build step.**
- **Third-party code gate:** before adding ANY crate, npm package or vendored JS file (e.g. a grid/drag library), name it, its publisher/maintainer, maturity, how it updates and what it can reach, then wait for Olivier's go. Mainstream, organisation-maintained crates only. `Cargo.lock` committed; vendored JS pinned to an exact version with its licence file.
- **No LLM, no metered API.**
- **Built with Fable 5.2** (Olivier's test of the model on a whole project).

## Run
<filled in as it gets built: cargo test / cargo run -- --demo / config file format>

## Deploy
Runs on the owner's home server, reachable over Tailscale only. Details in `.private/NOTES.md`.

## Gotchas
<one line per trap that cost time>
