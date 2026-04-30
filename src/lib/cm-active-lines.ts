// Shared state field tracking which line numbers the cursor occupies.
// Every "live preview" decoration (wikilinks, math, images, marker
// hides, table widgets) flips between raw and rendered based on this
// set, and they all read it from one place so we don't walk the
// selection ranges five times per transaction.
//
// In read pose the set is always empty: the cursor is hidden, so no
// line should expose its raw markup. Forge implements read pose via
// `EditorView.editable.of(false)` (a view-level facet), NOT
// `EditorState.readOnly` — we check both so wikilinks/math/widgets
// flip the moment the read toggle fires, instead of needing a second
// reconfigure to land.
//
// The field preserves the previous Set identity when membership is
// unchanged. That lets dependent state fields short-circuit on
// `oldLines === newLines` without doing a deep-compare.

import { EditorState, StateField } from "@codemirror/state";
import { EditorView } from "@codemirror/view";

const EMPTY: ReadonlySet<number> = new Set();

function inReadPose(state: EditorState): boolean {
  return state.readOnly || !state.facet(EditorView.editable);
}

function compute(state: EditorState): ReadonlySet<number> {
  if (inReadPose(state)) return EMPTY;
  const lines = new Set<number>();
  const doc = state.doc;
  for (const range of state.selection.ranges) {
    const s = doc.lineAt(range.from).number;
    const e = doc.lineAt(range.to).number;
    for (let l = s; l <= e; l++) lines.add(l);
  }
  return lines;
}

function setsEqual(a: ReadonlySet<number>, b: ReadonlySet<number>): boolean {
  if (a === b) return true;
  if (a.size !== b.size) return false;
  for (const v of a) if (!b.has(v)) return false;
  return true;
}

export const activeLinesField = StateField.define<ReadonlySet<number>>({
  create(state) {
    return compute(state);
  },
  update(value, tr) {
    if (!tr.docChanged && !tr.selection) {
      // Reconfigure dispatches fire without docChanged/selection. Check
      // both pose-affecting facets so a read-pose toggle drops the
      // active lines on the same transaction.
      const sameRO = tr.startState.readOnly === tr.state.readOnly;
      const sameEditable =
        tr.startState.facet(EditorView.editable) ===
        tr.state.facet(EditorView.editable);
      if (sameRO && sameEditable) return value;
    }
    const next = compute(tr.state);
    return setsEqual(value, next) ? value : next;
  },
});

export function isLineActive(
  active: ReadonlySet<number>,
  lineNumber: number,
): boolean {
  return active.has(lineNumber);
}

export function anyLineActive(
  active: ReadonlySet<number>,
  fromLine: number,
  toLine: number,
): boolean {
  if (active.size === 0) return false;
  for (let l = fromLine; l <= toLine; l++) if (active.has(l)) return true;
  return false;
}
