import { Address, Contract, TransactionBuilder, xdr } from 'stellar-sdk';
import { rpc, scValToNative, nativeToScVal } from '@stellar/stellar-sdk';

const CONTRACT_ID = process.env.PROPOSAL_CONTRACT_ID || '';

export interface Proposal {
  id: string;
  creator: string;
  title: string;
  description: string;
  voteCount: number;
  votes: Array<{ voter: string; supports: boolean }>;
}

export class ProposalClient {
  private contract: Contract;

  constructor(private rpcUrl: string, private sourceKeypair: any) {
    this.contract = new Contract(CONTRACT_ID);
  }

  async createProposal(title: string, description: string): Promise<Proposal> {
    const source = this.sourceKeypair.publicKey();
    const args = [
      nativeToScVal(source, { type: 'address' }),
      nativeToScVal(title, { type: 'string' }),
      nativeToScVal(description, { type: 'string' })
    ];

    const tx = new TransactionBuilder(this.sourceKeypair, {
      fee: '100',
      networkPassphrase: 'Test SDF Network ; September 2015'
    })
      .addOperation(this.contract.call('create_proposal', ...args))
      .setTimeout(30)
      .build();

    const signedTx = tx.sign(this.sourceKeypair);
    const result = await rpc.submitTransaction(signedTx, this.rpcUrl);

    if (!result.successful) {
      throw new Error('Transaction failed');
    }

    return this.parseProposal(result.returnValue);
  }

  async vote(proposalId: string, supports: boolean): Promise<void> {
    const args = [
      nativeToScVal(proposalId, { type: 'u64' }),
      nativeToScVal(this.sourceKeypair.publicKey(), { type: 'address' }),
      nativeToScVal(supports, { type: 'bool' })
    ];

    const tx = new TransactionBuilder(this.sourceKeypair, {
      fee: '100',
      networkPassphrase: 'Test SDF Network ; September 2015'
    })
      .addOperation(this.contract.call('vote', ...args))
      .setTimeout(30)
      .build();

    const signedTx = tx.sign(this.sourceKeypair);
    const result = await rpc.submitTransaction(signedTx, this.rpcUrl);

    if (!result.successful) {
      throw new Error('Vote transaction failed');
    }
  }

  private parseProposal(scVal: xdr.ScVal): Proposal {
    const native = scValToNative(scVal);
    return {
      id: native.id.toString(),
      creator: native.creator,
      title: native.title,
      description: native.description,
      voteCount: Number(native.vote_count),
      votes: native.votes.map((v: any) => ({
        voter: v[0],
        supports: v[1]
      }))
    };
  }
}
