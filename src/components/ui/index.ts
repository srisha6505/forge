// Barrel file for UI primitives. Import from "@/components/ui" (or
// "../ui") rather than reaching into individual primitive modules so
// the surface stays swappable.

export { default as GhostBtn } from "./GhostBtn";
export { default as SecondaryBtn } from "./SecondaryBtn";
export { default as PrimaryBtn } from "./PrimaryBtn";
export { default as SegCtrl } from "./SegCtrl";
export { default as Toggle } from "./Toggle";
export { default as InputField } from "./InputField";
export { default as Chip } from "./Chip";
export { default as StatusDot } from "./StatusDot";
export { default as Divider } from "./Divider";
export { default as Kbd } from "./Kbd";

export type { GhostBtnProps } from "./GhostBtn";
export type { SecondaryBtnProps } from "./SecondaryBtn";
export type { PrimaryBtnProps } from "./PrimaryBtn";
export type { SegCtrlProps, SegCtrlOption } from "./SegCtrl";
export type { ToggleProps } from "./Toggle";
export type { InputFieldProps } from "./InputField";
export type { ChipProps } from "./Chip";
export type { StatusDotProps, StatusDotVariant } from "./StatusDot";
export type { DividerProps } from "./Divider";
export type { KbdProps } from "./Kbd";
