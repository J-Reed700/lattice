import React from 'react';
import { useDrafting } from '../hooks/useDrafting';

export const DraftingView: React.FC = () => {
  const { state, startDrafting } = useDrafting();

  return (
    <div style={{ padding: '20px', border: '1px solid #ccc' }}>
      <h3>Lesson Drafting</h3>
      <button onClick={() => startDrafting('input', 'brief')}>Start Drafting</button>
      <div>Status: {state.status}</div>
      {state.status === 'drafting' && <div>Progress: {state.progress}</div>}
      {state.status === 'complete' && <div>Result: {JSON.stringify(state.result)}</div>}
      {state.status === 'error' && <div>Error: {state.error}</div>}
    </div>
  );
};
