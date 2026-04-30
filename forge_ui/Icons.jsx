/* global React */
// Lucide-style icons, stroke 1.8 per design.md §5
const I = (paths, { size = 16, stroke = 1.8, fill = "none" } = {}) =>
  React.createElement("svg", {
    width: size, height: size, viewBox: "0 0 24 24", fill, stroke: "currentColor",
    strokeWidth: stroke, strokeLinecap: "round", strokeLinejoin: "round",
    style: { display: "inline-block", verticalAlign: "middle", flexShrink: 0 }
  }, ...(Array.isArray(paths) ? paths : [paths]).map((d, i) =>
    typeof d === "string" ? React.createElement("path", { key: i, d }) : React.cloneElement(d, { key: i })
  ));

const Icons = {
  Files: (p) => I([
    "M20 7h-3a2 2 0 0 1-2-2V2", "M9 18a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h7l4 4v10a2 2 0 0 1-2 2z",
    "M3 7.6v12.8A1.6 1.6 0 0 0 4.6 22h9.8"
  ], p),
  Search: (p) => I([React.createElement("circle", { cx: 11, cy: 11, r: 8 }), "m21 21-4.3-4.3"], p),
  MessageSquare: (p) => I(["M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"], p),
  GitFork: (p) => I([React.createElement("circle", { cx: 12, cy: 18, r: 3 }), React.createElement("circle", { cx: 6, cy: 6, r: 3 }), React.createElement("circle", { cx: 18, cy: 6, r: 3 }), "M18 9v2c0 .6-.4 1-1 1H7c-.6 0-1-.4-1-1V9", "M12 12v3"], p),
  Terminal: (p) => I(["m4 17 6-6-6-6", "M12 19h8"], p),
  Settings: (p) => I([
    "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z",
    React.createElement("circle", { cx: 12, cy: 12, r: 3 })
  ], p),
  Mic: (p) => I([React.createElement("rect", { x: 9, y: 2, width: 6, height: 13, rx: 3 }), "M19 10v2a7 7 0 0 1-14 0v-2", "M12 19v3"], p),
  Moon: (p) => I("M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9z", p),
  Sun: (p) => I([React.createElement("circle", { cx: 12, cy: 12, r: 4 }), "M12 2v2", "M12 20v2", "m4.93 4.93 1.41 1.41", "m17.66 17.66 1.41 1.41", "M2 12h2", "M20 12h2", "m6.34 17.66-1.41 1.41", "m19.07 4.93-1.41 1.41"], p),
  ChevronRight: (p) => I("m9 18 6-6-6-6", p),
  ChevronDown: (p) => I("m6 9 6 6 6-6", p),
  ChevronUp: (p) => I("m18 15-6-6-6 6", p),
  X: (p) => I(["M18 6 6 18", "m6 6 12 12"], p),
  Plus: (p) => I(["M12 5v14", "M5 12h14"], p),
  File: (p) => I(["M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7z", "M14 2v6h6"], p),
  FileText: (p) => I(["M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7z", "M14 2v6h6", "M10 13H8", "M16 17H8", "M16 13h-2"], p),
  Folder: (p) => I("M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2z", p),
  FolderOpen: (p) => I(["m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2"], p),
  Check: (p) => I("M20 6 9 17l-5-5", p),
  Copy: (p) => I([React.createElement("rect", { x: 9, y: 9, width: 13, height: 13, rx: 2 }), "M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"], p),
  RotateCcw: (p) => I(["M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8", "M3 3v5h5"], p),
  ExternalLink: (p) => I(["M15 3h6v6", "M10 14 21 3", "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"], p),
  MoreHorizontal: (p) => I([React.createElement("circle", { cx: 12, cy: 12, r: 1 }), React.createElement("circle", { cx: 19, cy: 12, r: 1 }), React.createElement("circle", { cx: 5, cy: 12, r: 1 })], p),
  Send: (p) => I(["m22 2-7 20-4-9-9-4z", "m22 2-9.3 9.3"], p),
  PanelRight: (p) => I([React.createElement("rect", { x: 3, y: 3, width: 18, height: 18, rx: 2 }), "M15 3v18"], p),
  User: (p) => I([React.createElement("circle", { cx: 12, cy: 8, r: 5 }), "M20 21a8 8 0 0 0-16 0"], p),
  Bot: (p) => I([React.createElement("rect", { x: 3, y: 11, width: 18, height: 10, rx: 2 }), React.createElement("circle", { cx: 12, cy: 5, r: 2 }), "M12 7v4", "M8 16h0", "M16 16h0"], p),
  Zap: (p) => I("M13 2 3 14h9l-1 8 10-12h-9l1-8z", { ...p, fill: "none" }),
  Download: (p) => I(["M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4", "m7 10 5 5 5-5", "M12 15V3"], p),
  Trash2: (p) => I(["M3 6h18", "M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6", "M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2", "M10 11v6", "M14 11v6"], p),
  Key: (p) => I(["m15.5 7.5 2.3 2.3a1 1 0 0 0 1.4 0l2.1-2.1a1 1 0 0 0 0-1.4L19 4a1 1 0 0 0-1.4 0l-2.1 2.1a1 1 0 0 0 0 1.4z", "m2.2 21.8 3.1-3.1a1 1 0 0 0 0-1.4L3 15a1 1 0 0 0-1.4 0L.4 16.2a1 1 0 0 0 0 1.4l2.3 2.3"], { ...p, fill: "none" }),
  Globe: (p) => I([React.createElement("circle", { cx: 12, cy: 12, r: 10 }), "M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20", "M2 12h20"], p),
  SlidersHorizontal: (p) => I(["M21 4h-8", "M7 4H3", "M21 12h-4", "M11 12H3", "M21 20h-10", "M5 20H3", React.createElement("circle", { cx: 9, cy: 4, r: 2 }), React.createElement("circle", { cx: 15, cy: 12, r: 2 }), React.createElement("circle", { cx: 9, cy: 20, r: 2 })], p),
  Wrench: (p) => I("M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z", p),
  Keyboard: (p) => I([React.createElement("rect", { x: 2, y: 4, width: 20, height: 16, rx: 2 }), "M6 8h0", "M10 8h0", "M14 8h0", "M18 8h0", "M8 12h0", "M12 12h0", "M16 12h0", "M7 16h10"], p),
  Info: (p) => I([React.createElement("circle", { cx: 12, cy: 12, r: 10 }), "M12 16v-4", "M12 8h.01"], p),
  PenLine: (p) => I(["M12 20h9", "M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z"], p),
  BookOpen: (p) => I(["M12 7v14", "M2 3h6a4 4 0 0 1 4 4 4 4 0 0 1 4-4h6v14h-6a4 4 0 0 0-4 4 4 4 0 0 0-4-4H2z"], p),
  Eye: (p) => I(["M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0", React.createElement("circle", { cx: 12, cy: 12, r: 3 })], p),
  ListTree: (p) => I(["M21 12h-8", "M21 6h-8", "M21 18h-8", "M3 6v4c0 1.1.9 2 2 2h3", "M3 10v6c0 1.1.9 2 2 2h3"], p),
  History: (p) => I([React.createElement("circle", { cx: 12, cy: 12, r: 10 }), "M12 6v6l4 2", "M2 12h2"], { ...p, fill: "none" }),
  Link2: (p) => I(["M9 17H7A5 5 0 0 1 7 7h2", "M15 7h2a5 5 0 1 1 0 10h-2", "M8 12h8"], p),
  MessageSquarePlus: (p) => I(["M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z", "M12 7v6", "M9 10h6"], p),
  ArrowUpRight: (p) => I(["M7 17 17 7", "M7 7h10v10"], p),
  AlignLeft: (p) => I(["M21 6H3", "M15 12H3", "M17 18H3"], p),
};

window.Icons = Icons;
