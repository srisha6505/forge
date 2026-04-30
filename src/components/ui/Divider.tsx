import type { CSSProperties } from "react";

export interface DividerProps {
  style?: CSSProperties;
}

// 1px horizontal rule using the theme hr colour. Prototype renders this
// as a <div> (not <hr>) so flex gap rules apply cleanly; we keep that.
export default function Divider({ style }: DividerProps) {
  return (
    <div
      style={{
        height: 1,
        background: "var(--hr-color)",
        ...style,
      }}
    />
  );
}
