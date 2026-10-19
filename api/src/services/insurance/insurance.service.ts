import { randomUUID } from 'crypto';
import {
  ConflictError,
  NotFoundError,
  UnauthorizedError,
  ValidationError,
} from '../../utils/errors';

export type ClaimStatus = 'submitted' | 'approved' | 'denied' | 'disputed';
export type Trigger = 'oracle_failure' | 'contract_compromise';

export interface InsuranceProvider {
  id: string;
  address: string;
  name: string;
  kycStatus: 'pending' | 'approved' | 'rejected';
  collateral: number;
  availableCollateral: number;
  rating: number;
}

export interface InsurancePolicy {
  id: string;
  providerId: string;
  coverageAmount: number;
  premiumBps: number;
  durationDays: number;
  terms: string;
  coveredTriggers: Trigger[];
  exclusions: string[];
  active: boolean;
}

export interface PurchasedCoverage {
  id: string;
  policyId: string;
  lender: string;
  positionId: string;
  coverageAmount: number;
  premiumPaid: number;
  startsAt: number;
  expiresAt: number;
}

export interface InsuranceClaim {
  id: string;
  coverageId: string;
  trigger: Trigger;
  evidence: string;
  amount: number;
  status: ClaimStatus;
  submittedAt: number;
  resolvedAt?: number;
}

class InsuranceService {
  private providers = new Map<string, InsuranceProvider>();
  private policies = new Map<string, InsurancePolicy>();
  private coverages = new Map<string, PurchasedCoverage>();
  private claims = new Map<string, InsuranceClaim>();

  resetStore() {
    this.providers.clear();
    this.policies.clear();
    this.coverages.clear();
    this.claims.clear();
  }

  onboardProvider(input: Omit<InsuranceProvider, 'id' | 'availableCollateral' | 'rating'>) {
    if (input.collateral <= 0) throw new ValidationError('Provider collateral must be positive');
    const provider: InsuranceProvider = {
      ...input,
      id: randomUUID(),
      kycStatus: input.kycStatus || 'pending',
      availableCollateral: input.collateral,
      rating: 0,
    };
    this.providers.set(provider.id, provider);
    return provider;
  }

  updateKycStatus(providerId: string, status: 'pending' | 'approved' | 'rejected'): InsuranceProvider {
    const provider = this.providers.get(providerId);
    if (!provider) throw new NotFoundError('Provider not found');
    provider.kycStatus = status;
    return provider;
  }

  createPolicy(input: Omit<InsurancePolicy, 'id' | 'active'>, callerAddress?: string) {
    const provider = this.providers.get(input.providerId);
    if (!provider) throw new NotFoundError('Provider not found');
    if (callerAddress && provider.address !== callerAddress) {
      throw new UnauthorizedError('Caller does not own this insurance provider');
    }
    if (provider.kycStatus !== 'approved') {
      throw new ValidationError('Approved provider required');
    }
    if (input.coverageAmount <= 0 || input.coverageAmount > provider.availableCollateral) {
      throw new ValidationError('Coverage exceeds provider collateral');
    }
    const policy = { ...input, id: randomUUID(), active: true };
    this.policies.set(policy.id, policy);
    return policy;
  }

  listPolicies() {
    return [...this.policies.values()].filter((policy) => policy.active);
  }

  purchase(policyId: string, lender: string, positionId: string, requestedCoverage?: number) {
    if (!lender) throw new ValidationError('Lender address is required');
    if (!positionId) throw new ValidationError('Position ID is required');
    const policy = this.policies.get(policyId);
    if (!policy?.active) throw new ValidationError('Policy is unavailable');
    const provider = this.providers.get(policy.providerId);
    if (!provider) throw new NotFoundError('Provider not found');
    const coverageAmount = Math.min(
      requestedCoverage ?? policy.coverageAmount,
      policy.coverageAmount
    );
    if (coverageAmount <= 0 || coverageAmount > provider.availableCollateral) {
      throw new ValidationError('Insurer is insolvent');
    }
    const now = Date.now();
    const coverage: PurchasedCoverage = {
      id: randomUUID(),
      policyId,
      lender,
      positionId,
      coverageAmount,
      premiumPaid: (coverageAmount * policy.premiumBps) / 10_000,
      startsAt: now,
      expiresAt: now + policy.durationDays * 86_400_000,
    };
    provider.availableCollateral -= coverageAmount;
    this.coverages.set(coverage.id, coverage);
    return coverage;
  }

  submitClaim(
    coverageId: string,
    trigger: Trigger,
    evidence: string,
    amount: number,
    claimantAddress?: string
  ) {
    const coverage = this.coverages.get(coverageId);
    if (!coverage) throw new NotFoundError('Coverage not found');
    if (claimantAddress && coverage.lender !== claimantAddress) {
      throw new UnauthorizedError('Claimant does not own this coverage');
    }
    if (!evidence || typeof evidence !== 'string' || evidence.trim() === '') {
      throw new ValidationError('Valid evidence is required to submit a claim');
    }
    if (amount <= 0) {
      throw new ValidationError('Claim amount must be positive');
    }
    const policy = this.policies.get(coverage.policyId);
    if (!policy) throw new NotFoundError('Policy not found');

    const eligible = Date.now() <= coverage.expiresAt && policy.coveredTriggers.includes(trigger);
    const claim: InsuranceClaim = {
      id: randomUUID(),
      coverageId,
      trigger,
      evidence,
      amount: Math.min(amount, coverage.coverageAmount),
      status: eligible ? 'approved' : 'denied',
      submittedAt: Date.now(),
      resolvedAt: Date.now(),
    };
    this.claims.set(claim.id, claim);
    return claim;
  }

  disputeClaim(claimId: string, callerAddress?: string) {
    const claim = this.claims.get(claimId);
    if (!claim) throw new NotFoundError('Claim not found');
    if (callerAddress) {
      const coverage = this.coverages.get(claim.coverageId);
      if (!coverage || coverage.lender !== callerAddress) {
        throw new UnauthorizedError('Caller does not own this claim');
      }
    }
    if (claim.status !== 'denied') throw new ConflictError('Only denied claims can be disputed');
    claim.status = 'disputed';
    claim.resolvedAt = undefined;
    return claim;
  }

  dashboard(providerId?: string) {
    const claims = [...this.claims.values()].filter((claim) => {
      if (!providerId) return true;
      const coverage = this.coverages.get(claim.coverageId);
      return coverage && this.policies.get(coverage.policyId)?.providerId === providerId;
    });
    return {
      claims,
      totals: claims.reduce<Record<ClaimStatus, number>>(
        (a, c) => ({ ...a, [c.status]: a[c.status] + 1 }),
        { submitted: 0, approved: 0, denied: 0, disputed: 0 }
      ),
    };
  }

  calculatePremium(policyId: string, riskScore: number): {
    basePremiumBps: number;
    riskAdjustedPremiumBps: number;
    riskMultiplier: number;
    estimatedPremium: number;
    coverageAmount: number;
  } {
    const policy = this.policies.get(policyId);
    if (!policy) throw new NotFoundError('Policy not found');

    const basePremiumBps = policy.premiumBps;
    let riskMultiplier = 1.0;
    if (riskScore >= 900) riskMultiplier = 0.8;
    else if (riskScore >= 750) riskMultiplier = 1.0;
    else if (riskScore >= 600) riskMultiplier = 1.3;
    else if (riskScore >= 400) riskMultiplier = 1.8;
    else riskMultiplier = 2.5;

    const riskAdjustedPremiumBps = Math.round(basePremiumBps * riskMultiplier);
    const estimatedPremium = (policy.coverageAmount * riskAdjustedPremiumBps) / 10_000;

    return {
      basePremiumBps,
      riskAdjustedPremiumBps,
      riskMultiplier,
      estimatedPremium,
      coverageAmount: policy.coverageAmount,
    };
  }

  listProviders(): InsuranceProvider[] {
    return [...this.providers.values()];
  }

  getPolicy(policyId: string): InsurancePolicy | undefined {
    return this.policies.get(policyId);
  }

  listCoverages(lender?: string): PurchasedCoverage[] {
    const all = [...this.coverages.values()];
    return lender ? all.filter((c) => c.lender === lender) : all;
  }

  getAnalytics(): {
    totalProviders: number;
    activePolicies: number;
    totalCoverages: number;
    totalPremiumsCollected: number;
    totalCoverageIssued: number;
    claimsByStatus: Record<ClaimStatus, number>;
    avgPremiumBps: number;
  } {
    const activePolicies = this.listPolicies();
    const coverages = [...this.coverages.values()];
    const claims = [...this.claims.values()];

    const totalPremiumsCollected = coverages.reduce((sum, c) => sum + c.premiumPaid, 0);
    const totalCoverageIssued = coverages.reduce((sum, c) => sum + c.coverageAmount, 0);
    const avgPremiumBps = activePolicies.length > 0
      ? activePolicies.reduce((sum, p) => sum + p.premiumBps, 0) / activePolicies.length
      : 0;

    const claimsByStatus = claims.reduce<Record<ClaimStatus, number>>(
      (a, c) => ({ ...a, [c.status]: a[c.status] + 1 }),
      { submitted: 0, approved: 0, denied: 0, disputed: 0 }
    );

    return {
      totalProviders: this.providers.size,
      activePolicies: activePolicies.length,
      totalCoverages: coverages.length,
      totalPremiumsCollected,
      totalCoverageIssued,
      claimsByStatus,
      avgPremiumBps: Math.round(avgPremiumBps * 100) / 100,
    };
  }
}

export const insuranceService = new InsuranceService();
