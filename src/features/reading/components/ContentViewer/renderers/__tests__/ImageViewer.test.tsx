import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it } from 'vitest';

import { ImageViewer } from '../ImageViewer';

const image = () => screen.getByRole('img');

it('shows the file with a loading state until it has loaded', () => {
  render(<ImageViewer filePath="/garden/tomato.png" title="Tomato blight" />);
  expect(screen.getByRole('heading', { name: 'Tomato blight' })).toBeInTheDocument();
  expect(image()).toHaveAttribute('src', 'file:///garden/tomato.png');
  expect(image()).toHaveAttribute('alt', 'Tomato blight');
  expect(document.querySelector('.animate-spin')).not.toBeNull();
  fireEvent.load(image());
  expect(document.querySelector('.animate-spin')).toBeNull();
});

it('zooms within bounds, rotates and resets', () => {
  render(<ImageViewer filePath="/garden/leaf.png" />);
  expect(image()).toHaveAttribute('alt', 'Image');
  const zoomIn = screen.getByRole('button', { name: 'Zoom in' });
  const zoomOut = screen.getByRole('button', { name: 'Zoom out' });

  for (let i = 0; i < 20; i += 1) fireEvent.click(zoomIn);
  expect(screen.getByText('500%')).toBeInTheDocument();
  expect(zoomIn).toBeDisabled();
  for (let i = 0; i < 30; i += 1) fireEvent.click(zoomOut);
  expect(screen.getByText('25%')).toBeInTheDocument();
  expect(zoomOut).toBeDisabled();

  for (let i = 0; i < 5; i += 1) fireEvent.click(screen.getByRole('button', { name: 'Rotate' }));
  expect(image().style.transform).toBe('scale(0.25) rotate(90deg)');
  fireEvent.click(screen.getByRole('button', { name: 'Reset' }));
  expect(image().style.transform).toBe('scale(1) rotate(0deg)');
  expect(screen.getByText('100%')).toBeInTheDocument();
});

it('replaces the viewer with an error when the image cannot load', () => {
  render(<ImageViewer filePath="/garden/missing.png" />);
  fireEvent.error(image());
  expect(screen.getAllByText('Failed to load image')).toHaveLength(2);
  expect(screen.queryByRole('img')).not.toBeInTheDocument();
});
