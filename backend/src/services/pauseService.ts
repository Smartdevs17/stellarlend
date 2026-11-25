export interface PauseProposal {
  poolId: string;
  proposer: string;
  proposedAt: number;
  executableAt: number;
  executed: boolean;
  signatures: string[];
}

export class TimelockedPauseService {
  private proposals: Map<string, PauseProposal> = new Map();
  private readonly delayMs: number;
  private readonly requiredSignatures: number;

  constructor(delaySeconds: number = 3600, requiredSignatures: number = 2) {
    this.delayMs = delaySeconds * 1000;
    this.requiredSignatures = requiredSignatures;
  }

  public propose(poolId: string, proposer: string): PauseProposal {
    const now = Date.now();
    const proposal: PauseProposal = {
      poolId,
      proposer,
      proposedAt: now,
      executableAt: now + this.delayMs,
      executed: false,
      signatures: [proposer],
    };
    this.proposals.set(poolId, proposal);
    return proposal;
  }

  public signProposal(poolId: string, signer: string): boolean {
    const proposal = this.proposals.get(poolId);
    if (!proposal || proposal.executed) return false;
    if (!proposal.signatures.includes(signer)) {
      proposal.signatures.push(signer);
    }
    return true;
  }

  public canExecute(poolId: string): { ready: boolean; reason?: string } {
    const proposal = this.proposals.get(poolId);
    if (!proposal) return { ready: false, reason: 'Proposal not found' };
    if (proposal.executed) return { ready: false, reason: 'Already executed' };
    if (proposal.signatures.length < this.requiredSignatures) {
      return { ready: false, reason: `Insufficient signatures: ${proposal.signatures.length}/${this.requiredSignatures}` };
    }
    if (Date.now() < proposal.executableAt) {
      return { ready: false, reason: 'Timelock delay window has not expired' };
    }
    return { ready: true };
  }
}
