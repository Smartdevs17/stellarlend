import { Address } from '@stellar/stellar-sdk';

export interface SafelistRule {
  contractAddress: string;
  label: string;
  approvedAt: string;
  expiresAt?: string;
  active: boolean;
}

export class FlashloanSafelistManager {
  private safelist: Map<string, SafelistRule> = new Map();

  public registerContract(contractAddress: string, label: string, expiresAt?: string): void {
    if (!contractAddress || contractAddress.trim() === '') {
      throw new Error('Invalid contract address');
    }
    this.safelist.set(contractAddress, {
      contractAddress,
      label,
      approvedAt: new Date().toISOString(),
      expiresAt,
      active: true,
    });
  }

  public revokeContract(contractAddress: string): boolean {
    const entry = this.safelist.get(contractAddress);
    if (!entry) return false;
    entry.active = false;
    return true;
  }

  public isApproved(contractAddress: string): boolean {
    const entry = this.safelist.get(contractAddress);
    if (!entry || !entry.active) return false;
    if (entry.expiresAt && new Date(entry.expiresAt).getTime() < Date.now()) {
      return false;
    }
    return true;
  }

  public listApprovedContracts(): SafelistRule[] {
    return Array.from(this.safelist.values()).filter(rule => this.isApproved(rule.contractAddress));
  }
}
