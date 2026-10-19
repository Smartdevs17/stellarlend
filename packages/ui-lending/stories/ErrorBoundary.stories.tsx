import type { Meta, StoryObj } from '@storybook/react';
import React from 'react';
import { ErrorBoundary } from '../src/components/ErrorBoundary';
import { createTheme } from '../src/utils/theme';

const meta: Meta<typeof ErrorBoundary> = {
  title: 'Lending/ErrorBoundary',
  component: ErrorBoundary,
  parameters: { layout: 'centered' },
  tags: ['autodocs'],
};

export default meta;
type Story = StoryObj<typeof ErrorBoundary>;

function Broken({ message }: { message: string }): React.ReactElement {
  throw new Error(message);
}

export const NetworkError: Story = {
  args: { children: <Broken message="TypeError: Failed to fetch" /> },
};

export const WalletRejected: Story = {
  args: { children: <Broken message="User declined access" /> },
};

export const UnknownError: Story = {
  args: { children: <Broken message="Cannot read properties of undefined (reading 'value')" /> },
};

export const DarkMode: Story = {
  args: { children: <Broken message="Request timed out" />, theme: createTheme('dark') },
  parameters: { backgrounds: { default: 'dark' } },
};
