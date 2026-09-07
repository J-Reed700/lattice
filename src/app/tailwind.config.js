/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./websrc/**/*.{js,ts,jsx,tsx}",
  ],
  darkMode: 'selector', // matches the `.dark` class set by useApplyTheme
  theme: {
    extend: {
      fontFamily: {
        sans: ['Inter', '-apple-system', 'BlinkMacSystemFont', 'Segoe UI', 'system-ui', 'sans-serif'],
        mono: ['JetBrains Mono', 'Menlo', 'Monaco', 'Consolas', 'monospace'],
        serif: ['Source Serif 4', 'Charter', 'Iowan Old Style', 'Apple Garamond', 'Georgia', 'Cambria', 'Times New Roman', 'serif'],
      },
      colors: {
        bg: 'hsl(var(--bg))',
        surface: {
          DEFAULT: 'hsl(var(--surface))',
          raised: 'hsl(var(--surface-raised))',
        },
        border: {
          DEFAULT: 'hsl(var(--border-subtle))',
          subtle: 'hsl(var(--border-subtle))',
          default: 'hsl(var(--border-default))',
          strong: 'hsl(var(--border-strong))',
        },
        text: {
          primary: 'hsl(var(--text-primary))',
          secondary: 'hsl(var(--text-secondary))',
          tertiary: 'hsl(var(--text-tertiary))',
          muted: 'hsl(var(--text-muted))',
          disabled: 'hsl(var(--text-disabled))',
        },
        accent: {
          DEFAULT: 'hsl(var(--accent))',
          hover: 'hsl(var(--accent-hover))',
          muted: 'hsl(var(--accent-muted))',
          fg: 'hsl(var(--accent-fg))',
        },
        success: {
          DEFAULT: 'hsl(var(--success))',
          muted: 'hsl(var(--success-muted))',
          fg: 'hsl(var(--success-fg))',
        },
        warning: {
          DEFAULT: 'hsl(var(--warning))',
          muted: 'hsl(var(--warning-muted))',
          fg: 'hsl(var(--warning-fg))',
        },
        danger: {
          DEFAULT: 'hsl(var(--danger))',
          muted: 'hsl(var(--danger-muted))',
          fg: 'hsl(var(--danger-fg))',
        },
        ring: 'hsl(var(--ring))',
        overlay: 'hsl(var(--overlay))',
        chart: {
          '1': 'hsl(var(--chart-1))',
          '2': 'hsl(var(--chart-2))',
          '3': 'hsl(var(--chart-3))',
          '4': 'hsl(var(--chart-4))',
          '5': 'hsl(var(--chart-5))',
        },
      },
      fontSize: {
        'xxs': ['0.6875rem', { lineHeight: '1.45', letterSpacing: '0.02em', fontWeight: '500' }],
        'xs': ['0.75rem', { lineHeight: '1.5', letterSpacing: '0.01em', fontWeight: '400' }],
        'sm': ['0.875rem', { lineHeight: '1.55', letterSpacing: '0', fontWeight: '400' }],
        'base': ['1rem', { lineHeight: '1.6', letterSpacing: '0', fontWeight: '400' }],
        'lg': ['1.125rem', { lineHeight: '1.55', letterSpacing: '-0.005em', fontWeight: '500' }],
        'xl': ['1.25rem', { lineHeight: '1.4', letterSpacing: '-0.01em', fontWeight: '600' }],
        '2xl': ['1.5rem', { lineHeight: '1.3', letterSpacing: '-0.015em', fontWeight: '600' }],
        '3xl': ['1.875rem', { lineHeight: '1.2', letterSpacing: '-0.02em', fontWeight: '600' }],
      },
      borderRadius: {
        sm: 'var(--radius-sm)',
        md: 'var(--radius-md)',
        lg: 'var(--radius-lg)',
        full: 'var(--radius-full)',
      },
      boxShadow: {
        none: 'var(--shadow-none)',
        sm: 'var(--shadow-sm)',
        md: 'var(--shadow-md)',
      },
      transitionDuration: {
        fast: 'var(--duration-fast)',
        base: 'var(--duration-base)',
        slow: 'var(--duration-slow)',
      },
      transitionTimingFunction: {
        out: 'var(--ease-out)',
        in: 'var(--ease-in)',
        linear: 'var(--ease-linear)',
      },
    },
  },
  plugins: [require("tailwindcss-animate")],
}
