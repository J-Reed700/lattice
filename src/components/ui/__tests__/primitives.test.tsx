import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';

import { Button } from '../button';
import { Card } from '../card';
import { Input } from '../input';
import { PageHeader } from '../PageHeader';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../tabs';

describe('Button', () => {
  it('uses action tokens for primary CTA and compact h-8 rounded-lg defaults', () => {
    render(<Button>Save changes</Button>);
    const btn = screen.getByRole('button', { name: 'Save changes' });
    expect(btn.className).toContain('bg-action');
    expect(btn.className).toContain('text-action-fg');
    expect(btn.className).toContain('h-8');
    expect(btn.className).toContain('rounded-lg');
  });

  it('supports outline, secondary, ghost, destructive, and link variants', () => {
    const { rerender } = render(<Button variant="outline">Outline</Button>);
    expect(screen.getByRole('button').className).toContain('border-border-default');
    expect(screen.getByRole('button').className).toContain('shadow-control');

    rerender(<Button variant="secondary">Secondary</Button>);
    expect(screen.getByRole('button').className).toContain('shadow-control');

    rerender(<Button variant="destructive">Delete</Button>);
    expect(screen.getByRole('button').className).toContain('bg-danger');

    rerender(<Button variant="link">Learn more</Button>);
    expect(screen.getByRole('button').className).toContain('text-accent');
  });

  it('renders via asChild without breaking slot composition', () => {
    render(
      <Button asChild>
        <a href="#settings">Open settings</a>
      </Button>,
    );
    const link = screen.getByRole('link', { name: 'Open settings' });
    expect(link).toHaveAttribute('href', '#settings');
    expect(link.className).toContain('bg-action');
  });
});

describe('Input desktop density sizes', () => {
  it('defaults to h-8 rounded-lg and supports sm and lg sizes', () => {
    const { rerender } = render(<Input label="Query" />);
    expect(screen.getByLabelText('Query').className).toContain('h-8');
    expect(screen.getByLabelText('Query').className).toContain('rounded-lg');

    rerender(<Input label="Query" size="sm" />);
    expect(screen.getByLabelText('Query').className).toContain('h-7');

    rerender(<Input label="Query" size="lg" />);
    expect(screen.getByLabelText('Query').className).toContain('h-10');
  });
});

describe('Tabs variants', () => {
  it('defaults to segmented pill variant and switches panels', async () => {
    const user = userEvent.setup();
    render(
      <Tabs defaultValue="general">
        <TabsList>
          <TabsTrigger value="general">General</TabsTrigger>
          <TabsTrigger value="models">Models</TabsTrigger>
        </TabsList>
        <TabsContent value="general">General content</TabsContent>
        <TabsContent value="models">Models content</TabsContent>
      </Tabs>,
    );

    const tablist = screen.getByRole('tablist');
    expect(tablist).toHaveAttribute('data-variant', 'pill');
    const generalTrigger = screen.getByRole('tab', { name: 'General' });
    expect(generalTrigger).toHaveAttribute('data-variant', 'pill');
    expect(generalTrigger.className).toContain('data-[state=active]:bg-surface');
    expect(generalTrigger.className).toContain('data-[state=active]:shadow-sm');
    expect(generalTrigger.className).toContain('data-[state=active]:ring-1');
    expect(screen.getByText('General content')).toBeInTheDocument();

    await user.click(screen.getByRole('tab', { name: 'Models' }));
    expect(screen.getByText('Models content')).toBeInTheDocument();
  });

  it('propagates underline variant from Tabs or TabsList to triggers', () => {
    render(
      <Tabs variant="underline" defaultValue="overview">
        <TabsList>
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <TabsTrigger value="sources">Sources</TabsTrigger>
        </TabsList>
        <TabsContent value="overview">Overview panel</TabsContent>
      </Tabs>,
    );

    expect(screen.getByRole('tablist')).toHaveAttribute('data-variant', 'underline');
    const trigger = screen.getByRole('tab', { name: 'Overview' });
    expect(trigger).toHaveAttribute('data-variant', 'underline');
    expect(trigger.className).toContain('data-[state=active]:border-accent');
  });

  it('allows TabsList to set underline variant independently', () => {
    render(
      <Tabs defaultValue="one">
        <TabsList variant="underline">
          <TabsTrigger value="one">One</TabsTrigger>
        </TabsList>
        <TabsContent value="one">Panel one</TabsContent>
      </Tabs>,
    );

    expect(screen.getByRole('tablist')).toHaveAttribute('data-variant', 'underline');
    expect(screen.getByRole('tab', { name: 'One' })).toHaveAttribute('data-variant', 'underline');
  });
});

describe('Card and PageHeader Option A styling', () => {
  it('renders Card with bg-surface, border-border-subtle, and shadow-sm', () => {
    render(<Card data-testid="card">Card body</Card>);
    const card = screen.getByTestId('card');
    expect(card.className).toContain('bg-surface');
    expect(card.className).toContain('border-border-subtle');
    expect(card.className).toContain('shadow-sm');
  });

  it('renders PageHeader title with crisp architectural sans typography', () => {
    render(<PageHeader title="Library" meta="3 documents" />);
    const heading = screen.getByRole('heading', { level: 1, name: 'Library' });
    expect(heading.className).toContain('font-sans');
    expect(heading.className).toContain('font-semibold');
    expect(heading.className).toContain('tracking-[-0.025em]');
  });
});
