import type { Meta, StoryObj } from '@storybook/react';
import React from 'react';
import { PositionMonitor } from '../src/components/PositionMonitor';
import { createTheme } from '../src/utils/theme';

const meta: Meta<typeof PositionMonitor> = {
  title: 'Lending/PositionMonitor',
  component: PositionMonitor,
  parameters: { layout: 'padded' },
  tags: ['autodocs'],
};

export default meta;
type Story = StoryObj<typeof PositionMonitor>;

const positions = [
  {
    id: 'usdc',
    asset: 'USD Coin',
    symbol: 'USDC',
    supplied: 5000,
    borrowed: 0,
    supplyApy: 4.18,
    borrowApy: 6.92,
    collateralFactor: 0.8,
    price: 1,
  },
  {
    id: 'xlm',
    asset: 'Stellar Lumens',
    symbol: 'XLM',
    supplied: 2000,
    borrowed: 12000,
    supplyApy: 2.1,
    borrowApy: 5.4,
    collateralFactor: 0.7,
    price: 0.12,
  },
];

export const Live: Story = {
  args: { positions, lastUpdated: Date.now(), onRefresh: () => undefined },
};

export const AtRisk: Story = {
  args: {
    positions: [{ ...positions[1]!, supplied: 20000, borrowed: 11000 }],
    lastUpdated: Date.now(),
  },
};

export const Delayed: Story = {
  args: { positions, lastUpdated: Date.now() - 120000, error: "We couldn't reach StellarLend. Check your connection and try again." },
};

export const Loading: Story = {
  args: { positions: [], isLoading: true },
};

export const Empty: Story = {
  args: { positions: [], lastUpdated: Date.now() },
};

export const DarkMode: Story = {
  args: { positions, lastUpdated: Date.now(), theme: createTheme('dark') },
  parameters: { backgrounds: { default: 'dark' } },
};

export const Mobile: Story = {
  args: { positions, lastUpdated: Date.now(), onRefresh: () => undefined },
  parameters: { viewport: { defaultViewport: 'mobile1' } },
};
