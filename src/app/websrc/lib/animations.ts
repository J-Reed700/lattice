import type { Variants } from 'framer-motion';

/**
 * Framer Motion Animation Variants
 *
 * Reusable animation configurations for smooth, professional page transitions
 * and component animations throughout the Recall desktop app.
 *
 * **Design Philosophy**:
 * - Smooth, subtle animations that don't distract from content
 * - Spring-based physics for natural, organic motion
 * - Fast enough to feel responsive (< 300ms)
 * - Accessible and respects user preferences (prefers-reduced-motion)
 *
 * @see https://www.framer.com/motion/animation/##variants
 */

/**
 * Page Transition Variant
 *
 * Used for smooth page-to-page navigation with fade and slide effects.
 * Optimized for React Router route transitions.
 *
 * **Animation Flow**:
 * 1. `initial` - Page starts invisible and slightly to the right
 * 2. `animate` - Page fades in and slides to center position
 * 3. `exit` - Page fades out and slides slightly to the left
 *
 * **Performance Notes**:
 * - Uses `opacity` and `x` transforms (GPU-accelerated)
 * - Spring physics for smooth, natural deceleration
 * - Duration ~250ms for snappy but smooth feel
 *
 * @example
 * ```tsx
 * import { motion } from 'framer-motion';
 * import { pageTransition } from '@/lib/animations';
 *
 * <motion.div
 *   variants={pageTransition}
 *   initial="initial"
 *   animate="animate"
 *   exit="exit"
 * >
 *   <YourPage />
 * </motion.div>
 * ```
 */
export const pageTransition: Variants = {
  initial: {
    opacity: 0,
    y: 8,
  },
  animate: {
    opacity: 1,
    y: 0,
    transition: {
      type: 'spring',
      stiffness: 300,
      damping: 30,
      mass: 0.8,
      duration: 0.25,
    },
  },
  exit: {
    opacity: 0,
    y: -4,
    transition: {
      type: 'spring',
      stiffness: 300,
      damping: 30,
      mass: 0.8,
      duration: 0.2,
    },
  },
};

/**
 * Fade In Variant
 *
 * Simple fade-in animation for loading states and appearing content.
 *
 * @example
 * ```tsx
 * <motion.div variants={fadeIn} initial="initial" animate="animate">
 *   <Content />
 * </motion.div>
 * ```
 */
export const fadeIn: Variants = {
  initial: {
    opacity: 0,
  },
  animate: {
    opacity: 1,
    transition: {
      duration: 0.3,
      ease: 'easeOut',
    },
  },
};

/**
 * Scale Up Variant
 *
 * Subtle scale-up animation for modal dialogs and popovers.
 *
 * @example
 * ```tsx
 * <motion.div variants={scaleUp} initial="initial" animate="animate">
 *   <Dialog />
 * </motion.div>
 * ```
 */
export const scaleUp: Variants = {
  initial: {
    opacity: 0,
    scale: 0.95,
  },
  animate: {
    opacity: 1,
    scale: 1,
    transition: {
      type: 'spring',
      stiffness: 400,
      damping: 25,
      duration: 0.2,
    },
  },
  exit: {
    opacity: 0,
    scale: 0.95,
    transition: {
      duration: 0.15,
      ease: 'easeIn',
    },
  },
};

/**
 * Slide Up Variant
 *
 * Slide-up animation for bottom sheets, notifications, and toast messages.
 *
 * @example
 * ```tsx
 * <motion.div variants={slideUp} initial="initial" animate="animate">
 *   <Notification />
 * </motion.div>
 * ```
 */
export const slideUp: Variants = {
  initial: {
    opacity: 0,
    y: 20,
  },
  animate: {
    opacity: 1,
    y: 0,
    transition: {
      type: 'spring',
      stiffness: 350,
      damping: 25,
      duration: 0.25,
    },
  },
  exit: {
    opacity: 0,
    y: 10,
    transition: {
      duration: 0.15,
      ease: 'easeIn',
    },
  },
};

/**
 * Stagger Children Variant
 *
 * Parent container variant that staggers the animation of child elements.
 * Use with child variants like `fadeIn` or `slideUp`.
 *
 * @example
 * ```tsx
 * <motion.div variants={staggerChildren} initial="initial" animate="animate">
 *   <motion.div variants={fadeIn}>Child 1</motion.div>
 *   <motion.div variants={fadeIn}>Child 2</motion.div>
 *   <motion.div variants={fadeIn}>Child 3</motion.div>
 * </motion.div>
 * ```
 */
export const staggerChildren: Variants = {
  initial: {},
  animate: {
    transition: {
      staggerChildren: 0.05,
      delayChildren: 0.1,
    },
  },
};

/**
 * Fade In Up Variant
 *
 * Combines fade and slide-up animations for smooth reveal effects.
 * Used for dashboard components, stat cards, and content sections.
 *
 * **Animation Flow**:
 * - `hidden` - Element starts invisible and slightly below final position
 * - `visible` - Element fades in and slides up to final position
 *
 * **Performance Notes**:
 * - Uses GPU-accelerated `opacity` and `y` transforms
 * - Spring physics for natural, organic motion
 * - Fast duration (~250ms) for responsive feel
 *
 * @example
 * ```tsx
 * <motion.div variants={fadeInUp} initial="hidden" animate="visible">
 *   <StatCard />
 * </motion.div>
 * ```
 */
export const fadeInUp: Variants = {
  hidden: {
    opacity: 0,
    y: 12,
  },
  visible: {
    opacity: 1,
    y: 0,
    transition: {
      type: 'spring',
      stiffness: 350,
      damping: 25,
      mass: 0.8,
      duration: 0.25,
    },
  },
};

/**
 * Stagger Container Variant
 *
 * Parent container that orchestrates staggered animations for child elements.
 * Works with child variants that use `hidden`/`visible` states (e.g., fadeInUp).
 *
 * **Animation Flow**:
 * - Children animate sequentially with 50ms delay between each
 * - 100ms initial delay before first child animates
 * - Creates cascading reveal effect
 *
 * **Usage Pattern**:
 * - Apply to parent container
 * - Children must use variants with `hidden`/`visible` states
 * - Each child automatically inherits parent's animation state
 *
 * @example
 * ```tsx
 * <motion.div variants={staggerContainer} initial="hidden" animate="visible">
 *   <motion.div variants={fadeInUp}>Item 1</motion.div>
 *   <motion.div variants={fadeInUp}>Item 2</motion.div>
 *   <motion.div variants={fadeInUp}>Item 3</motion.div>
 * </motion.div>
 * ```
 */
export const staggerContainer: Variants = {
  hidden: {},
  visible: {
    transition: {
      staggerChildren: 0.05,
      delayChildren: 0.1,
    },
  },
};

/**
 * Scale In Variant
 *
 * Subtle scale and fade animation for empty states and placeholder content.
 * Slightly different from `scaleUp` with softer scaling for gentler emphasis.
 *
 * **Animation Flow**:
 * - `hidden` - Element starts invisible and slightly smaller (90% scale)
 * - `visible` - Element fades in and scales to full size
 *
 * **Performance Notes**:
 * - GPU-accelerated `opacity` and `scale` transforms
 * - Spring physics with moderate stiffness for smooth feel
 * - Medium duration (~300ms) for gentle, welcoming effect
 *
 * @example
 * ```tsx
 * <motion.div variants={scaleIn} initial="hidden" animate="visible">
 *   <EmptyState />
 * </motion.div>
 * ```
 */
export const scaleIn: Variants = {
  hidden: {
    opacity: 0,
    scale: 0.9,
  },
  visible: {
    opacity: 1,
    scale: 1,
    transition: {
      type: 'spring',
      stiffness: 300,
      damping: 25,
      mass: 0.8,
      duration: 0.3,
    },
  },
};

/**
 * Rotate In Variant
 *
 * Playful rotation and fade animation for icons and decorative elements.
 * Adds subtle rotational motion for engaging, dynamic feel.
 *
 * **Animation Flow**:
 * - `hidden` - Element starts invisible with slight counter-clockwise rotation (-10°)
 * - `visible` - Element fades in while rotating to upright position
 *
 * **Performance Notes**:
 * - GPU-accelerated `opacity` and `rotate` transforms
 * - Spring physics with lower stiffness for bouncy, playful motion
 * - Medium duration (~300ms) for noticeable but not distracting effect
 *
 * **Best Used For**:
 * - Icon containers in empty states
 * - Decorative elements
 * - Success/completion indicators
 *
 * @example
 * ```tsx
 * <motion.div variants={rotateIn} initial="hidden" animate="visible">
 *   <SearchIcon />
 * </motion.div>
 * ```
 */
export const rotateIn: Variants = {
  hidden: {
    opacity: 0,
    rotate: -10,
  },
  visible: {
    opacity: 1,
    rotate: 0,
    transition: {
      type: 'spring',
      stiffness: 200,
      damping: 15,
      mass: 0.8,
      duration: 0.3,
    },
  },
};
