// Blocking and dependency-free: apply the last persisted preference before paint.
// The backend remains authoritative; useApplyTheme refreshes this cache on load.
(() => {
  let preference = 'system';
  try {
    const saved = localStorage.getItem('lattice-theme');
    if (saved === 'light' || saved === 'dark') preference = saved;
  } catch {
    // Storage can be unavailable. The system preference still works.
  }
  const theme = preference === 'system'
    ? (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light')
    : preference;
  const root = document.documentElement;
  root.dataset.theme = theme;
  root.classList.toggle('dark', theme === 'dark');
  root.style.colorScheme = theme;
})();
