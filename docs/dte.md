# Decision provenance — DTE

Ruggine keeps the *why* behind its code as a DTE (Decision Tree Engineering) tree, the same
way Solenoid does. The tool is `tools/dte.py` and the vendored rules are `dte-rules/`, both
copied from Solenoid; Solenoid's `docs/dte.md` explains the local patches the tool carries
and the practice on top of DTE's rules, and that practice applies here unchanged.

- The tree lives at `tree/decisions/` (rings `A/`, `B/`, ...), with `inbox/` and `outbox/`,
  and `tree/` opens as one Obsidian vault beside `tree/specs/`.
- Rings: A is the author's (goals), B is the orchestrating agent, C and deeper are subagents
  (`dte.cfg` authority).
- `python3 tools/dte.py validate --as B` must print `OK` before a push.
- Leaves cite as `[[<ID>]] name`; the first alias is the name.
