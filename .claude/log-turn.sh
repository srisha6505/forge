#!/usr/bin/env bash
# Stop-hook helper: appends the latest turn (last user msg + last assistant text)
# from the session transcript to .claude/conv-log.md so future resumes can
# read the conversation back even when the in-context history has been compacted.
#
# stdin: Stop-hook JSON (we read .transcript_path from it)
# Reads only the tail of the transcript so it stays cheap on huge jsonls.

set -u
LOG=/home/code/Production/forge/.claude/conv-log.md

tp=$(jq -r '.transcript_path // empty' 2>/dev/null)
[ -z "$tp" ] || [ ! -f "$tp" ] && exit 0

# Slurp only the tail so this stays cheap on a 150 MB jsonl.
turn=$(tail -n 500 "$tp" 2>/dev/null | jq -s -r '
  def text_of:
    if type == "string" then .
    else (map(select(.type == "text")) | map(.text) | join("\n"))
    end;
  def visible_user_text:
    .message.content | text_of
    | select(test("^<(command-|local-command|system-)") | not)
    | select(length > 0);
  def assistant_text:
    .message.content
    | if type == "array"
        then (map(select(.type == "text")) | map(.text) | join("\n"))
        else empty
      end
    | select(length > 0);
  ([.[] | select(.type == "user") | visible_user_text] | last // "") as $u |
  ([.[] | select(.type == "assistant") | assistant_text] | last // "") as $a |
  if ($u == "" and $a == "") then empty
  else "### User\n\n\($u)\n\n### Claude\n\n\($a)\n"
  end' 2>/dev/null)

[ -z "$turn" ] && exit 0

ts=$(date -Iseconds)
{
  printf '\n---\n## %s\n\n%s' "$ts" "$turn"
} >> "$LOG"
