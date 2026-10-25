# Manyproxy

Manyproxt is a minimal application that uses diffrent vpn cores (xray-core, signbox, etc...) and uses theor capabilities to run 1 or many vpn configurations and expose them as a proxy on a local or global ip on diffrent ports to solve the problems of accessing diffrent ip addresses witout using remote socks or http proxies for the apps that only support those formats and openup posibilities.

## A note from Mahyar

I like ambitious ideas, simple systems, and software that feels obvious. Do not preserve complexity just because it already exists. Do not introduce machinery because it looks architecturally impressive. Understand the real constraint, then fight for the smallest model that makes the correct behavior unsurprising.

Channel both "measure twice, cut once" and "yagni". Fight scope creep. Try to honor the dev's intent in both a minimal and realistic fashion.

The rest of this document is meant to help you navigate the codebase and make changes effectively. Think of these instructions less as "hard rules", more as "good defaults". The developer's preferences should be able to override anything here.

### Performance without compromise

Lots of apps have gotten bogged down with bad tech decisions and "slop". i have not, and i'm proud of the performance of Manyproxy. We regularly audit for performance regressions iften caused by animations causing gpu spikes, lists being hard to render, and more. Make sure all changes are considerate of performance impact.

### General Principles

- use gpui-kit and rust skills when needed

## A small glossary

We need to be on the same page with terminology. When communicating, use this language:

- **you** means the agent reading this file and changing Manyproxy.
- **me, i, us** mean Mahyar and the people building Manyproxy. These are who you are talking to now.
- **user** means the person using Manyproxy.
- **project** means an environment-local workspace record rooted at a directory.

## Hit every surface

The most common defect in this repo is a change that works on the path you tested and is missing everywhere else. Before calling ui work done, walk this list and say which entries applied:

- **Entry points.** A behavior reachable from the chat view is usually also reachable from Settings, the command palette, and a keybinding. Fixing one is not fixing the feature.
- **Contracts.** Anything crossing the wire.
- **Reverse states.** If you added a way in, add the way out and the way to see it. Snooze needs unsnooze. Close needs reopen. A one-way door is a bug.
- **Docs.** Check whether the change makes existing guidance inaccurate. Apply the documentation before adding anything.

## Branch Names

Use a short branch name of at most three words, separated by hyphens. Do not use slashes or type prefixes such as `feat/` or `fix/`.

Examples: `importing-config`, `fix-scroll-state`, `add-x-core`.

## Commits and PR Titles

Use conventional commit-style messages and PR titles: `type(scope): summary`.

Valid types are `feat`, `fix`, `docs`, `chore`, `refactor`, and `test`. Scopes are optional; use the affected package or area when helpful, e.g. `core`, `manyproxy`, `app`, `desktop`.

Examples: `fix(list): simplify toggle styling`, `docs: update contributing guide`, `chore: regenerate types`.

## Verifying

## Documentation

Most code changes do not need an internal documentation change. Agents can read the code.

- `docs/internals/` is for architectural decisions and their reasons, constraints that span components, and implementation traps that are hard to discover from the source. Before adding a paragraph, ask what a maintainer would get wrong without it. If reading the relevant code answers the question, leave it out.
- Do not document every feature, enumerate fields or methods, narrate control flow, maintain file catalogs, or append PR summaries. Types, tests, and code already record the implementation. The glossary defines shared vocabulary; it is not a feature index.
- Keep a local implementation explanation in a nearby code comment. Use an internal doc when the reasoning crosses boundaries or needs context the code cannot carry well. Link to the relevant source instead of copying it.
- When a documented decision or constraint changes, rewrite or remove the affected text. Do not append another account of the new behavior. A new internal page needs a distinct, durable reason to exist.
- `docs/user/` helps users accomplish tasks. Give each major feature a concise section explaining what it does, how to start, and anything unintuitive. A settings path is useful; descriptions of visible buttons, icons, layouts, animations, or every UI state are not. Before adding text, ask what task or decision it helps the user with.
- Keep user docs in the shipped product's voice, without implementation details or contributor tooling. Update the relevant feature section when how to use it changes. A UI tweak does not need a documentation entry, and a new control does not need its own page.
- `docs/operations/` holds maintainer setup, release, and debugging procedures. Keep instructions for operating an installed Manyproxy server in the user guides.

## Plans and work artifacts

- Do not commit implementation plans, research notes, or agent scratch files. Keep temporary working material outside the worktree. `.plans/` is gitignored only as a safety net for legacy tooling.
- Track active maintainer work in the GitHub issue or project item that owns it. External proposals follow `CONTRIBUTING.md` and belong in Ideas discussions.
- A merged PR is the implementation record. Close or update its tracking item when the work lands; do not preserve a second checklist in the repository.

## How it works

Architecture and its constraints: `docs/internals/overview.md`. Glossary: `docs/internals/glossary.md`

## Where code lives

## Taste

- Complexity belongs at the adapter boundary. Orchestration stays pure, UI stays dumb.
- Comments say why: a constraint, an external quirk, or an invariant the code can't show. Don't restate names or signatures, narrate steps, or record history. `no-low-value-comments` catches the mechanical cases.
- If a rule here fights the task in front of you, say so loudly and get a human sign-off before breaking it.

## Additional tips

- Don't verify with browsers or computer use unless the user explicitly agrees or requests it.
- Security is important, but should not be over-indexed on, especially for dev mode/maintainer-only features.
