# CONVCORRECT — why I keep losing context, and how it's fixed

## The bug from the user's POV

> "I `/resume` the same chat (named `[build-forge]`), but every restart Claude
> starts from zero. No context, no memory of what we were doing. Feels like
> talking to an Alzheimer's patient."

Confirmed. This is real, not user error. `/resume` selects the right transcript
file but the conversation does not load into the model's working context.

## Root cause

The on-disk session transcript for this conversation is **152 MB** of JSONL
(13,511 message lines, 20 days of turns). The Opus 4.7 (1M context) window
holds roughly 4 MB of plain text. So when `/resume` loads the file, the harness
must aggressively compact 38× more transcript than fits — and on a session
this big, "compact" effectively means "drop almost everything."

The system prompt the harness injects every turn literally says:

> The system will automatically compress prior messages in your conversation
> as it approaches context limits.

On a 152 MB session, that compression is total. There's no human-readable
summary left for me to read on resume. I boot blank, then have to re-derive
the project state by reading repo files (INDEX.md, CHANGES.md, git log).

## Why memory + tasks weren't enough on their own

The auto-memory system at `~/.claude/projects/-home-code-Production-forge/memory/`
and the persistent task list (#64–#68) DO survive across sessions — they were
already loaded when this session booted. They give me **project context**
(architecture, conventions, what tasks exist).

What they don't give me is **conversation context**: what was just said, what
decision was just made, what "no like the literal context of this chat" refers
to. Memory is for durable facts; it isn't where turn-by-turn dialogue belongs.

## The fix (now in place)

A `Stop` hook in `.claude/settings.json` runs `.claude/log-turn.sh` after every
turn. The script:

1. Reads the live transcript path from the hook's stdin JSON.
2. Tails the last 500 lines of the JSONL (cheap even at 152 MB).
3. Extracts the latest user message and my latest text reply with `jq`,
   skipping system-reminder tags, command wrappers, tool calls, and thinking.
4. Appends them with a timestamp + separator to `.claude/conv-log.md`.

`MEMORY.md` now points at `conv-log.md` so future sessions auto-load the file
on boot. The conv-log grows as plain markdown, never compacted, always readable.

## What you should do

1. **Stop resuming the 152 MB session.** It will only get worse. Start a fresh
   session. The new session boots with auto-memory + task list + conv-log +
   CHANGES.md — that's enough for me to pick up exactly where we are.
2. If you want belt-and-braces, archive the giant transcript:
   `mv ~/.claude/projects/-home-code-Production-forge/71428f40-*.jsonl  ~/.claude/projects/-home-code-Production-forge/_archive/`
   so `/resume` can't accidentally pick it again.
3. Periodically `/clear` within a session to keep the on-disk JSONL from
   ballooning per-tool-call. The conv-log persists separately.

## Caveat — first reload

Hooks are watched per directory. The watcher only watches directories that
already had a `settings.json` when the current session started. This session
started without one, so the new Stop hook **won't fire in this session** until
you open `/hooks` once (which reloads config) or restart Claude Code. After
that, every Stop event appends to `.claude/conv-log.md`.

## Files this introduced

- `.claude/settings.json` — Stop hook registration
- `.claude/log-turn.sh` — extraction + append script (executable)
- `.claude/conv-log.md` — the growing per-turn log
- `~/.claude/projects/-home-code-Production-forge/memory/project_conv_persistence.md`
  — memory entry pointing back here
- `CONVCORRECT.md` — this file
