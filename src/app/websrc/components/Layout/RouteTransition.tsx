/**
 * RouteTransition
 *
 * Wraps the React Router <Outlet /> with a Framer Motion fade+slide so
 * navigating between major sections (Home, Files, Chat, Settings, etc.)
 * feels organic rather than flash-cut.
 *
 * Pulled out of the Layout because it needs its own motion.div container
 * keyed by location.pathname to make AnimatePresence detect route changes.
 *
 * Motion is intentionally short and subtle — 180ms fade, 8px y-translate.
 * Anything bigger feels like an animation; anything shorter feels like a
 * bug. Reduced-motion users get the fade only via Framer Motion's
 * built-in respect for the media query.
 */

import { motion } from 'framer-motion';
import { Outlet, useLocation } from 'react-router-dom';

const PAGE_VARIANTS = {
  initial: { opacity: 0, y: 8 },
  animate: { opacity: 1, y: 0 },
  exit: { opacity: 0, y: -4 },
};

const PAGE_TRANSITION = {
  duration: 0.18,
  ease: [0.22, 1, 0.36, 1] as const, // matches --ease-out token
};

export function RouteTransition() {
  const location = useLocation();
  return (
    <motion.div
      key={location.pathname}
      variants={PAGE_VARIANTS}
      initial="initial"
      animate="animate"
      exit="exit"
      transition={PAGE_TRANSITION}
      className="flex-1 flex flex-col overflow-hidden"
    >
      <Outlet />
    </motion.div>
  );
}
