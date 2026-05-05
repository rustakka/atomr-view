# ai-skills/

Skills for AI coding assistants working on **projects that depend on
atomr-view** — not for editing atomr-view itself. They follow the standard
`SKILL.md` + frontmatter convention used by Claude Code, Claude Agent
SDK, and other agentic tools.

These skills provide domain-specific knowledge for building actor-based
UIs, managing the UI bridge, and leveraging Python for view logic.

## What's here

| Skill | Use when… |
|---|---|
| `atomr-view-design` | Authoring `WindowActor` or `RegionActor` — handling scenes, patches, and input routing. |
| `atomr-view-python` | Using the Python bindings for declarative UI and scene manipulation. |
| `atomr-view-troubleshooting` | Debugging UI bridge stalls, reconciliation errors, or backend mismatches. |

## Installing

Pick the path that matches your assistant. The skills themselves are
vendor-neutral `SKILL.md` files — only the install mechanism differs.

### Claude Code (recommended: marketplace)

If you use Claude Code, install via the plugin marketplace — this
keeps the skills updated as atomr-view releases, with no manual copy step:

```text
/plugin marketplace add rustakka/atomr-view
/plugin install atomr-view-ai-skills@atomr-view
```

Skills auto-activate based on the `description` frontmatter — no need
to invoke them explicitly.

### Gemini CLI

Gemini CLI reads `GEMINI.md`. Point Gemini at the skills:

```markdown
<!-- in GEMINI.md -->
For atomr-view work, load the relevant skill from
`ai-skills/skills/<name>/SKILL.md` before editing.
```

## Authoring conventions

- **One job per skill.** Router into docs + examples for one task.
- **Defer to source-of-truth docs.** Link to `README.md` and `docs/*.md`.
- **Vendor-neutral.** Describe atomr-view, not the runtime loading the skill.
- **Frontmatter.** Begins with `---` containing `name` and `description`.
