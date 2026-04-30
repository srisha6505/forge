import type { ChangeEvent, CSSProperties } from "react";

export interface InputFieldProps {
  value?: string;
  placeholder?: string;
  readOnly?: boolean;
  type?: string;
  style?: CSSProperties;
  onChange?: (v: string) => void;
}

// 32px single-line input, neutral form-field background. `onChange`
// receives the unwrapped string rather than the synthetic event so the
// caller doesn't have to deal with e.target.value every time.
export default function InputField({
  value,
  placeholder,
  readOnly,
  type = "text",
  style,
  onChange,
}: InputFieldProps) {
  return (
    <input
      type={type}
      value={value}
      placeholder={placeholder}
      readOnly={readOnly}
      onChange={
        onChange
          ? (e: ChangeEvent<HTMLInputElement>) => onChange(e.target.value)
          : undefined
      }
      style={{
        height: 32,
        padding: "0 10px",
        borderRadius: "var(--radius-s)",
        background: "var(--background-modifier-form-field)",
        border: "1px solid var(--background-modifier-border)",
        color: "var(--text-normal)",
        fontSize: "var(--font-ui-medium)",
        outline: "none",
        width: "100%",
        ...style,
      }}
    />
  );
}
