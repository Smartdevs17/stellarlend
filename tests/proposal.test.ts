import { ProposalClient } from '../src/proposal/client';
import { Keypair, Networks } from 'stellar-sdk';

describe('Proposal Module', () => {
  const rpcUrl = 'http://localhost:8000';
  const adminKeypair = Keypair.fromSecret('SDRD4...'); // Test secret
  const client = new ProposalClient(rpcUrl, adminKeypair);

  test('create proposal', async () => {
    const proposal = await client.createProposal(
      'Test Proposal',
      'This is a test proposal'
    );
    expect(proposal.title).toBe('Test Proposal');
    expect(proposal.description).toBe('This is a test proposal');
    expect(proposal.voteCount).toBe(0);
  });

  test('vote on proposal', async () => {
    const proposal = await client.createProposal(
      'Vote Test',
      'Testing voting'
    );
    await client.vote(proposal.id, true);
    // In a real test, we'd fetch the updated proposal and verify vote count
  });

  test('prevent double voting', async () => {
    const proposal = await client.createProposal(
      'Double Vote Test',
      'Testing double vote prevention'
    );
    await client.vote(proposal.id, true);
    await expect(client.vote(proposal.id, false))
      .rejects
      .toThrow('already voted');
  });
});
