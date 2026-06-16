/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{vue,js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      fontFamily: {
        sans: ['Inter', 'sans-serif'],
      },
      colors: {
        primary: {
          DEFAULT: '#FF6B35', // Existing accent, kept as primary
          hover: '#FF8659',
          active: '#E55A2B',
        },
        dark: {
          950: '#09090b', // Zinc 950
          900: '#18181b', // Zinc 900
          800: '#27272a', // Zinc 800
          700: '#3f3f46', // Zinc 700
        },
        surface: {
          DEFAULT: '#18181b',
          hover: '#27272a',
        }
      },
      boxShadow: {
        'glass': '0 8px 32px 0 rgba(0, 0, 0, 0.37)',
      },
      backdropBlur: {
        'xs': '2px',
      },
      keyframes: {
        indeterminate: {
          '0%':   { transform: 'translateX(-100%) scaleX(0.5)' },
          '50%':  { transform: 'translateX(100%) scaleX(0.5)' },
          '100%': { transform: 'translateX(300%) scaleX(0.5)' },
        },
      },
      animation: {
        indeterminate: 'indeterminate 1.5s ease-in-out infinite',
      },
    },
  },
  darkMode: 'media',
  plugins: [],
};

