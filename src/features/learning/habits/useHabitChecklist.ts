import { useCallback, useEffect, useState } from 'react';

import { CHECKLIST_ITEMS, HABIT_CHECKLIST_STORAGE_KEY } from './types';

import type { HabitChecklistState } from './types';

const CHECKLIST_EVENT = 'lattice:habit-checklist-update';

function getStoredChecklist(): HabitChecklistState {
  if (typeof window === 'undefined') return {};
  try {
    const raw = window.localStorage.getItem(HABIT_CHECKLIST_STORAGE_KEY);
    if (!raw) return {};
    return JSON.parse(raw) as HabitChecklistState;
  } catch {
    return {};
  }
}

export function useHabitChecklist() {
  const [checklist, setChecklist] = useState<HabitChecklistState>(getStoredChecklist);

  useEffect(() => {
    const handleUpdate = () => {
      setChecklist(getStoredChecklist());
    };
    window.addEventListener('storage', handleUpdate);
    window.addEventListener(CHECKLIST_EVENT, handleUpdate);
    return () => {
      window.removeEventListener('storage', handleUpdate);
      window.removeEventListener(CHECKLIST_EVENT, handleUpdate);
    };
  }, []);

  const toggleItem = useCallback((id: string) => {
    setChecklist((prev) => {
      const next: HabitChecklistState = { ...prev, [id]: !prev[id] };
      try {
        window.localStorage.setItem(HABIT_CHECKLIST_STORAGE_KEY, JSON.stringify(next));
        window.dispatchEvent(new CustomEvent(CHECKLIST_EVENT));
      } catch {
        // Storage might be unavailable
      }
      return next;
    });
  }, []);

  const setItemState = useCallback((id: string, checked: boolean) => {
    setChecklist((prev) => {
      const next: HabitChecklistState = { ...prev, [id]: checked };
      try {
        window.localStorage.setItem(HABIT_CHECKLIST_STORAGE_KEY, JSON.stringify(next));
        window.dispatchEvent(new CustomEvent(CHECKLIST_EVENT));
      } catch {
        // Storage might be unavailable
      }
      return next;
    });
  }, []);

  const resetAll = useCallback(() => {
    try {
      window.localStorage.removeItem(HABIT_CHECKLIST_STORAGE_KEY);
      window.dispatchEvent(new CustomEvent(CHECKLIST_EVENT));
    } catch {
      // Storage might be unavailable
    }
    setChecklist({});
  }, []);

  const completedCount = CHECKLIST_ITEMS.filter((item) => Boolean(checklist[item.id])).length;
  const totalCount = CHECKLIST_ITEMS.length;

  return {
    checklist,
    completedCount,
    isComplete: completedCount === totalCount && totalCount > 0,
    resetAll,
    setItemState,
    toggleItem,
    totalCount,
  };
}
