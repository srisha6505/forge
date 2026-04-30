/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      fontFamily: {
        // Defer to the CSS vars App.tsx writes from settings on boot.
        // Without this, tailwind preflight pinned `body` to a hardcoded
        // Inter stack and beat the user's body_font / interface_font.
        sans: ["var(--font-interface)", "system-ui", "sans-serif"],
        mono: ["var(--font-monospace)", "Menlo", "monospace"],
      },
      colors: {
        panel: {
          50: "#fafafa",
          100: "#f4f4f5",
          200: "#e4e4e7",
          300: "#d4d4d8",
          700: "#3f3f46",
          800: "#27272a",
          900: "#18181b",
        },
      },
    },
  },
  plugins: [],
};
