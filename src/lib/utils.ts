import { type ClassValue, clsx } from "clsx"
import { extendTailwindMerge } from "tailwind-merge"

// tailwind-merge only knows the stock scale. Without this it reads our custom
// sizes (`text-ui`, `text-xxs`) as colours and drops them next to `text-text-*`.
const twMerge = extendTailwindMerge({
  extend: {
    theme: {
      text: ['xxs', 'ui'],
      shadow: ['sheet', 'control', 'action'],
      radius: ['xs'],
    },
  },
})

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}
