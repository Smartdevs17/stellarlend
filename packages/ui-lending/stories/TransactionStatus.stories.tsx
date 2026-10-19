import type { Meta, StoryObj } from '@storybook/react';
import React from 'react';
import { TransactionStatus } from '../src/components/TransactionStatus';
import { createTheme } from '../src/utils/theme';

const meta: Meta<typeof TransactionStatus> = {
  title: 'Lending/TransactionStatus',
  component: TransactionStatus,
  parameters: { layout: 'centered' },
  tags: ['autodocs'],
};

export default meta;
type Story = StoryObj<typeof TransactionStatus>;

const hash = '3389e9f0f1a65f19736cacf544c2e825313e8447f569233bb8db39aa607c8889';
const explorerUrl = (txHash: string) => `https://stellar.expert/explorer/testnet/tx/${txHash}`;

export const Confirming: Story = {
  args: { tracking: { hash, state: 'pending', startedAt: Date.now() }, explorerUrl },
};

export const Confirmed: Story = {
  args: { tracking: { hash, state: 'success', ledger: 1234567, startedAt: Date.now() }, explorerUrl },
};

export const Failed: Story = {
  args: { tracking: { hash, state: 'failed', error: 'The transaction failed on-chain.', startedAt: Date.now() }, explorerUrl },
};

export const TimedOut: Story = {
  args: { tracking: { hash, state: 'timeout', startedAt: Date.now() }, explorerUrl },
};

export const DarkMode: Story = {
  args: { tracking: { hash, state: 'pending', startedAt: Date.now() }, theme: createTheme('dark') },
  parameters: { backgrounds: { default: 'dark' } },
};
