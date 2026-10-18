import { useState } from 'react';

import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { CONSERVATIVE_DEFAULT, presetGeometry, type VisionDeclaration, type VisionGeometry } from '@/utils/visionGeometry';

import { VisionDeclarationEditor } from './VisionDeclarationEditor';

/** Saves what the editor reports, the way the settings round trip does. */
function Harness({ initial, defaultGeometry = null, onChange }: {
  initial: VisionDeclaration | null;
  defaultGeometry?: VisionGeometry | null;
  onChange: (next: VisionDeclaration) => void;
}) {
  const [value, setValue] = useState(initial);
  return (
    <VisionDeclarationEditor
      value={value}
      defaultGeometry={defaultGeometry}
      onChange={(next) => { onChange(next); setValue(next); }}
    />
  );
}

const preview = () => screen.getByTestId('vision-cost-preview').textContent;
const choice = () => screen.getByRole('combobox', { name: 'Vision' }) as HTMLSelectElement;

describe('VisionDeclarationEditor', () => {
  it('starts off for an undeclared model with no default, and says pictures are not shown', () => {
    render(<Harness initial={null} onChange={vi.fn()} />);
    expect(choice().value).toBe('off');
    expect(preview()).toBe('Pictures are not shown to this model.');
    expect(screen.queryByRole('button', { name: 'Advanced' })).toBeNull();
  });

  it('starts a cloud model on its provider default and prices the preview picture with it', () => {
    render(<Harness initial={null} defaultGeometry={presetGeometry('open_ai')} onChange={vi.fn()} />);
    expect(choice().value).toBe('open_ai');
    // 85 + 6 tiles × 170, as the backend's tile arithmetic charges it.
    expect(preview()).toBe('A 1280×960 photo costs about 1,105 tokens on this model.');
  });

  it('offers the conservative default for a geometry nobody knows', async () => {
    const onChange = vi.fn();
    render(<Harness initial={null} onChange={onChange} />);
    const option = screen.getByRole('option', { name: 'Conservative default (up to 1,026 tokens per picture)' }) as HTMLOptionElement;
    await userEvent.selectOptions(choice(), option);
    expect(onChange).toHaveBeenLastCalledWith({ enabled: true, geometry: CONSERVATIVE_DEFAULT });
    expect(preview()).toBe('A 1280×960 photo costs about 974 tokens on this model.');
  });

  it('a preset fills the geometry fields', async () => {
    const onChange = vi.fn();
    render(<Harness initial={null} onChange={onChange} />);
    await userEvent.selectOptions(choice(), 'qwen25_vl');
    expect(onChange).toHaveBeenLastCalledWith({ enabled: true, geometry: presetGeometry('qwen25_vl') });
    expect(preview()).toBe('A 1280×960 photo costs about 1,273 tokens on this model.');

    await userEvent.click(screen.getByRole('button', { name: 'Advanced' }));
    expect(screen.getByRole('radio', { name: 'Grid' })).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByLabelText('Token side (px)')).toHaveValue(28);
    expect(screen.getByLabelText('Max pixels')).toHaveValue(1280 * 28 * 28);
  });

  it('turning vision off keeps the geometry for when it comes back on', async () => {
    const onChange = vi.fn();
    render(<Harness initial={{ enabled: true, geometry: presetGeometry('gemini') }} onChange={onChange} />);
    await userEvent.selectOptions(choice(), 'off');
    expect(onChange).toHaveBeenLastCalledWith({ enabled: false, geometry: presetGeometry('gemini') });
  });

  it('Custom edits a field and saves the edited geometry', async () => {
    const onChange = vi.fn();
    render(<Harness initial={{ enabled: true, geometry: presetGeometry('gemma_siglip') }} onChange={onChange} />);
    await userEvent.selectOptions(choice(), 'custom');
    const tokens = screen.getByLabelText('Tokens');
    await userEvent.clear(tokens);
    await userEvent.type(tokens, '280');
    fireEvent.blur(tokens);
    expect(onChange).toHaveBeenLastCalledWith({ enabled: true, geometry: { kind: 'fixed', side: 896, tokens: 280 } });
    expect(choice().value).toBe('custom');
    expect(preview()).toBe('A 1280×960 photo costs about 280 tokens on this model.');
  });

  it('switching the geometry kind starts from that kind of encoder', async () => {
    const onChange = vi.fn();
    render(<Harness initial={{ enabled: true, geometry: presetGeometry('gemma_siglip') }} onChange={onChange} />);
    await userEvent.click(screen.getByRole('button', { name: 'Advanced' }));
    await userEvent.click(screen.getByRole('radio', { name: 'Tiles' }));
    expect(onChange).toHaveBeenLastCalledWith({ enabled: true, geometry: presetGeometry('open_ai') });
    expect(screen.getByLabelText('Max tiles')).toHaveValue(16);
  });

  it('refuses a geometry the settings check would reject, without saving it', async () => {
    const onChange = vi.fn();
    render(<Harness initial={{ enabled: true, geometry: CONSERVATIVE_DEFAULT }} onChange={onChange} />);
    await userEvent.click(screen.getByRole('button', { name: 'Advanced' }));
    const side = screen.getByLabelText('Token side (px)');
    await userEvent.clear(side);
    await userEvent.type(side, '0');
    fireEvent.blur(side);
    expect(onChange).not.toHaveBeenCalled();
    expect(screen.getByRole('alert')).toHaveTextContent('a grid geometry needs a token side above 0');
  });
});
