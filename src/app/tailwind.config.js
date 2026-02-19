/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./websrc/**/*.{js,ts,jsx,tsx}",
  ],
  darkMode: ['selector', "class"], // Use [data-theme="dark"] selector for dark mode
  theme: {
  	extend: {
  		fontFamily: {
  			sans: ['Inter', '-apple-system', 'BlinkMacSystemFont', 'Segoe UI', 'Roboto', 'sans-serif'],
  			mono: ['JetBrains Mono', 'Menlo', 'Monaco', 'Consolas', 'monospace'],
  		},
  		letterSpacing: {
  			display: '-0.025em',
  			heading: '-0.02em',
  		},
  		colors: {
  			brand: {
  				'50': '#f0f9ff',
  				'100': '#e0f2fe',
  				'200': '#bae6fd',
  				'300': '#7dd3fc',
  				'400': '#38bdf8',
  				'500': '#0ea5e9',
  				'600': '#0284c7',
  				'700': '#0369a1',
  				'800': '#075985',
  				'900': '#0c4a6e',
  				'950': '#082f49'
  			},
  			accent: {
  				purple: '#a855f7',
  				pink: '#ec4899',
  				orange: '#f97316',
  				emerald: '#10b981',
  				DEFAULT: 'hsl(var(--accent))',
  				foreground: 'hsl(var(--accent-foreground))'
  			},
  			background: 'hsl(var(--background))',
  			foreground: 'hsl(var(--foreground))',
  			card: {
  				DEFAULT: 'hsl(var(--card))',
  				foreground: 'hsl(var(--card-foreground))'
  			},
  			popover: {
  				DEFAULT: 'hsl(var(--popover))',
  				foreground: 'hsl(var(--popover-foreground))'
  			},
  			primary: {
  				DEFAULT: 'hsl(var(--primary))',
  				foreground: 'hsl(var(--primary-foreground))'
  			},
  			secondary: {
  				DEFAULT: 'hsl(var(--secondary))',
  				foreground: 'hsl(var(--secondary-foreground))'
  			},
  			muted: {
  				DEFAULT: 'hsl(var(--muted))',
  				foreground: 'hsl(var(--muted-foreground))'
  			},
  			destructive: {
  				DEFAULT: 'hsl(var(--destructive))',
  				foreground: 'hsl(var(--destructive-foreground))'
  			},
  			border: 'hsl(var(--border))',
  			input: 'hsl(var(--input))',
  			ring: 'hsl(var(--ring))',
  			chart: {
  				'1': 'hsl(var(--chart-1))',
  				'2': 'hsl(var(--chart-2))',
  				'3': 'hsl(var(--chart-3))',
  				'4': 'hsl(var(--chart-4))',
  				'5': 'hsl(var(--chart-5))'
  			}
  		},
  		fontSize: {
  			'display-2xl': 'clamp(3rem, 6vw + 1rem, 5rem)',
  			'display-xl': 'clamp(2.5rem, 5vw + 1rem, 4rem)',
  			'display-lg': 'clamp(2rem, 4vw + 0.5rem, 3.5rem)',
  			'display-md': 'clamp(1.75rem, 3vw + 0.5rem, 3rem)',
  			'display-sm': 'clamp(1.5rem, 2.5vw + 0.5rem, 2.5rem)',
  			'heading-xl': 'clamp(1.5rem, 2vw + 0.5rem, 2.25rem)',
  			'heading-lg': 'clamp(1.25rem, 1.5vw + 0.5rem, 2rem)',
  			'heading-md': 'clamp(1.125rem, 1vw + 0.5rem, 1.5rem)',
  			'heading-sm': 'clamp(1rem, 0.5vw + 0.5rem, 1.25rem)',
  			'body-xl': 'clamp(1.125rem, 0.5vw + 0.75rem, 1.25rem)',
  			'body-lg': 'clamp(1rem, 0.25vw + 0.75rem, 1.125rem)',
  			'body-md': 'clamp(0.875rem, 0.25vw + 0.75rem, 1rem)',
  			'body-sm': 'clamp(0.75rem, 0.25vw + 0.625rem, 0.875rem)',
  			caption: 'clamp(0.625rem, 0.25vw + 0.5rem, 0.75rem)'
  		},
  		spacing: {
  			'fluid-xs': 'clamp(0.5rem, 1vw, 0.75rem)',
  			'fluid-sm': 'clamp(0.75rem, 1.5vw, 1rem)',
  			'fluid-md': 'clamp(1rem, 2vw, 1.5rem)',
  			'fluid-lg': 'clamp(1.5rem, 3vw, 2rem)',
  			'fluid-xl': 'clamp(2rem, 4vw, 3rem)',
  			'fluid-2xl': 'clamp(3rem, 6vw, 4rem)'
  		},
  		boxShadow: {
  			glow: '0 0 20px rgba(14, 165, 233, 0.3)',
  			'elevation-1': '0 1px 3px rgba(0, 0, 0, 0.06), 0 1px 2px rgba(0, 0, 0, 0.08)',
  			'elevation-2': '0 4px 6px rgba(0, 0, 0, 0.07), 0 2px 4px rgba(0, 0, 0, 0.06)',
  			'elevation-4': '0 20px 25px rgba(0, 0, 0, 0.1), 0 10px 10px rgba(0, 0, 0, 0.04)'
  		},
  		backdropBlur: {
  			xs: '2px'
  		},
  		keyframes: {
  			shimmer: {
  				'0%': {
  					backgroundPosition: '-1000px 0'
  				},
  				'100%': {
  					backgroundPosition: '1000px 0'
  				}
  			},
  			'fade-in': {
  				'0%': {
  					opacity: '0'
  				},
  				'100%': {
  					opacity: '1'
  				}
  			},
  			fadeIn: {
  				'0%': { opacity: '0', transform: 'translateY(10px)' },
  				'100%': { opacity: '1', transform: 'translateY(0)' },
  			},
  			'glow-pulse': {
  				'0%, 100%': { opacity: '0.4' },
  				'50%': { opacity: '0.8' },
  			},
  		},
  		animation: {
  			shimmer: 'shimmer 2s infinite linear',
  			'fade-in': 'fade-in 0.2s ease-out',
  			fadeIn: 'fadeIn 0.3s ease-in',
  			'glow-pulse': 'glow-pulse 3s ease-in-out infinite',
  		},
  		transitionTimingFunction: {
  			'bounce-in': 'cubic-bezier(0.68, -0.55, 0.265, 1.55)',
  			smooth: 'cubic-bezier(0.4, 0, 0.2, 1)'
  		},
  		borderRadius: {
  			lg: 'var(--radius)',
  			md: 'calc(var(--radius) - 2px)',
  			sm: 'calc(var(--radius) - 4px)'
  		}
  	}
  },
  plugins: [require("tailwindcss-animate")],
}
