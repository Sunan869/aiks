/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        aiks: {
          50: "#f0f9ff",
          100: "#e0f2fe",
          500: "#0ea5e9",
          600: "#0284c7",
          700: "#0369a1",
        },
      },
    },
  },
  // Preserve component-layer selectors even when variants are selected dynamically.
  safelist: [
    "aiks-button", "aiks-button--sm", "aiks-button--md",
    "aiks-button--primary", "aiks-button--secondary", "aiks-button--outline",
    "aiks-button--ghost", "aiks-button--danger",
    "aiks-field-input", "aiks-field-select", "aiks-field-date",
  ],
  plugins: [],
};
