/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "media",
  theme: {
    extend: {
      fontFamily: {
        sans: [
          "ui-sans-serif",
          "-apple-system",
          "BlinkMacSystemFont",
          "SF Pro Text",
          "SF Pro Display",
          "Segoe UI",
          "system-ui",
          "sans-serif",
        ],
      },
      colors: {
        accent: {
          DEFAULT: "#7C3AED",
          light: "#EDE9FE",
          dark: "#6D28D9",
        },
      },
    },
  },
  plugins: [],
};
