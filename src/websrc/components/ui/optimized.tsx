/**
 * Optimized versions of UI components with React.memo
 * Use these for lists and frequently re-rendered components
 */

import { memo, type ComponentPropsWithoutRef } from 'react';

import { Button, type ButtonProps } from './button';
import { Card } from './card';
import { Checkbox } from './Checkbox';
import { Input } from './input';
import { Select } from './select';
import { Switch } from './switch';

import type * as SelectPrimitive from '@radix-ui/react-select';
import type * as SwitchPrimitives from '@radix-ui/react-switch';


// Extended prop types using ComponentPropsWithoutRef
interface ExtendedButtonProps extends ButtonProps {
  loading?: boolean;
}

type SelectProps = ComponentPropsWithoutRef<typeof SelectPrimitive.Root>;
interface ExtendedSelectProps extends SelectProps {
  options?: Array<{ label: string; value: string }>;
  onChange?: (value: string) => void;
}

type SwitchProps = ComponentPropsWithoutRef<typeof SwitchPrimitives.Root>;
interface ExtendedSwitchProps extends SwitchProps {
  label?: string;
}

// Memoized Button
export const MemoButton = memo(Button, (prevProps: ExtendedButtonProps, nextProps: ExtendedButtonProps) => (
    prevProps.children === nextProps.children &&
    prevProps.variant === nextProps.variant &&
    prevProps.size === nextProps.size &&
    prevProps.disabled === nextProps.disabled &&
    prevProps.loading === nextProps.loading &&
    prevProps.onClick === nextProps.onClick
  ));
MemoButton.displayName = 'MemoButton';

// Memoized Input
export const MemoInput = memo(Input, (prevProps, nextProps) => (
    prevProps.value === nextProps.value &&
    prevProps.placeholder === nextProps.placeholder &&
    prevProps.disabled === nextProps.disabled &&
    prevProps.onChange === nextProps.onChange
  ));
MemoInput.displayName = 'MemoInput';

// Memoized Select
export const MemoSelect = memo(Select, (prevProps: ExtendedSelectProps, nextProps: ExtendedSelectProps) => (
    prevProps.value === nextProps.value &&
    prevProps.options === nextProps.options &&
    prevProps.disabled === nextProps.disabled &&
    prevProps.onChange === nextProps.onChange
  ));
MemoSelect.displayName = 'MemoSelect';

// Memoized Checkbox
export const MemoCheckbox = memo(Checkbox, (prevProps, nextProps) => (
    prevProps.checked === nextProps.checked &&
    prevProps.disabled === nextProps.disabled &&
    prevProps.onCheckedChange === nextProps.onCheckedChange
  ));
MemoCheckbox.displayName = 'MemoCheckbox';

// Memoized Switch
export const MemoSwitch = memo(Switch, (prevProps: ExtendedSwitchProps, nextProps: ExtendedSwitchProps) => (
    prevProps.checked === nextProps.checked &&
    prevProps.label === nextProps.label &&
    prevProps.disabled === nextProps.disabled &&
    prevProps.onCheckedChange === nextProps.onCheckedChange
  ));
MemoSwitch.displayName = 'MemoSwitch';

// Memoized Card
export const MemoCard = memo(Card, (prevProps, nextProps) => (
    prevProps.children === nextProps.children &&
    prevProps.className === nextProps.className
  ));
MemoCard.displayName = 'MemoCard';

export const Optimized = {
  Button: MemoButton,
  Input: MemoInput,
  Select: MemoSelect,
  Checkbox: MemoCheckbox,
  Switch: MemoSwitch,
  Card: MemoCard,
};
