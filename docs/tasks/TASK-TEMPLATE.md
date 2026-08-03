# T-NNN — <title>

Phase: <n>  ·  Depends on: <T-NNN | none>  ·  Parallel-safe with: <list, or "all others in this phase">

## Goal

One paragraph. What this task delivers, and what "done" looks like from outside — a behaviour, not
a diff.

## Seam

The exact signature from `docs/SEAMS.md` this fills in, quoted. **Do not change it.** If it is
wrong, that is a seam-change PR first — see `CONTRIBUTING.md`.

## Owns

Paths this task may create or edit. Disjoint from every task that can run at the same time.

## Must not touch

Paths owned by others, listed explicitly so the boundary is unambiguous.

## Fixtures

Which fixture files this is tested against, and what each must demonstrate.

## Done criteria

The literal commands that must pass, and the assertions that must hold.

## Notes

Protocol traps and design constraints relevant to this task. Link to `docs/PROTOCOL-NOTES.md`
rather than restating it.
