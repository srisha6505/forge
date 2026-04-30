import { useState, type CSSProperties } from "react";
import { ChevronDown } from "./ui/Icons";

export interface Heading {
  level: 1 | 2 | 3 | 4 | 5 | 6;
  text: string;
  lineNumber?: number;
}

interface Props {
  title: string;
  headings: Heading[];
  onHeadingClick?: (heading: Heading) => void;
}

// Fixed-width Table of Contents panel mounted on the LEFT of the
// editor scroll area when tocOpen is true. Renders all six heading
// levels with progressive indent. A collapse chevron appears on any
// heading that has at least one strictly-deeper heading immediately
// following it (until the next sibling-or-shallower heading), and
// hides those descendants when toggled.
export default function TOCPanel({ title, headings, onHeadingClick }: Props) {
  const [collapsed, setCollapsed] = useState<Record<number, boolean>>({});

  const toggle = (i: number) =>
    setCollapsed((c) => ({ ...c, [i]: !c[i] }));

  // Show a chevron only when the immediately-following heading is
  // strictly deeper. Anything else means no children to fold.
  const hasChildren: boolean[] = headings.map((h, i) => {
    const next = headings[i + 1];
    return !!next && next.level > h.level;
  });

  // Compute hidden flag: a heading is hidden if any of its ancestors
  // (any strictly shallower heading earlier in the list, walking outward
  // through each shallower-than-current level we encounter) is collapsed.
  const hidden: boolean[] = headings.map((_h, i) => {
    let needLevel = headings[i].level;
    for (let j = i - 1; j >= 0; j--) {
      if (headings[j].level < needLevel) {
        if (collapsed[j]) return true;
        needLevel = headings[j].level;
      }
    }
    return false;
  });

  const rowStyle = (paddingLeft: number): CSSProperties => ({
    height: 28,
    display: "flex",
    alignItems: "center",
    gap: 4,
    paddingLeft,
    paddingRight: 8,
    color: "var(--text-muted)",
    cursor: "pointer",
    transition: "background var(--motion-duration-fast) var(--motion-ease)",
  });

  return (
    <div
      style={{
        width: 220,
        minWidth: 220,
        background: "var(--background-primary-alt)",
        borderRight: "1px solid var(--background-modifier-border)",
        overflowY: "auto",
        padding: "12px 0",
        fontSize: "var(--font-ui-small)",
      }}
    >
      <div
        style={{
          padding: "0 12px 8px",
          fontSize: "var(--font-ui-smaller)",
          fontWeight: 600,
          color: "var(--text-muted)",
          textTransform: "uppercase",
          letterSpacing: "0.04em",
        }}
      >
        Contents
      </div>
      <div
        style={{
          padding: "4px 12px",
          fontWeight: 600,
          color: "var(--text-normal)",
          cursor: "pointer",
          transition: "background var(--motion-duration-fast) var(--motion-ease)",
        }}
        onMouseEnter={(e) => {
          e.currentTarget.style.background = "var(--background-modifier-hover)";
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.background = "transparent";
        }}
      >
        {title}
      </div>
      {headings.map((h, i) => {
        if (hidden[i]) return null;
        const isCollapsed = !!collapsed[i];
        const showChevron = hasChildren[i];
        const paddingLeft = 12 + (h.level - 1) * 16;
        const isDeep = h.level >= 3;
        return (
          <div key={i}>
            <div
              onClick={() => {
                if (showChevron) toggle(i);
                onHeadingClick?.(h);
              }}
              style={rowStyle(paddingLeft)}
              onMouseEnter={(e) => {
                e.currentTarget.style.background =
                  "var(--background-modifier-hover)";
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.background = "transparent";
              }}
            >
              {showChevron ? (
                <span
                  style={{
                    width: 12,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    flexShrink: 0,
                    transition:
                      "transform var(--motion-duration-fast) var(--motion-ease)",
                    transform: isCollapsed ? "rotate(-90deg)" : "rotate(0)",
                  }}
                >
                  <ChevronDown size={10} />
                </span>
              ) : (
                // Spacer keeps text columns aligned with chevron rows.
                <span style={{ width: 12, flexShrink: 0 }} />
              )}
              <span
                style={{
                  flex: 1,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                  fontWeight: isDeep ? 400 : 500,
                }}
              >
                {h.text}
              </span>
            </div>
          </div>
        );
      })}
    </div>
  );
}
