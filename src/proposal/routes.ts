import express from 'express';
import { ProposalClient } from './client';

const router = express.Router();
const client = new ProposalClient(
  process.env.RPC_URL || 'http://localhost:8000',
  { publicKey: () => process.env.ADMIN_PUBLIC_KEY || '', secret: () => process.env.ADMIN_SECRET || '' }
);

router.post('/proposals', async (req, res) => {
  try {
    const { title, description } = req.body;
    if (!title || !description) {
      return res.status(400).json({ error: 'Title and description required' });
    }
    const proposal = await client.createProposal(title, description);
    res.status(201).json(proposal);
  } catch (error) {
    res.status(500).json({ error: 'Failed to create proposal' });
  }
});

router.post('/proposals/:id/vote', async (req, res) => {
  try {
    const { id } = req.params;
    const { supports } = req.body;
    if (typeof supports !== 'boolean') {
      return res.status(400).json({ error: 'supports must be boolean' });
    }
    await client.vote(id, supports);
    res.status(200).json({ success: true });
  } catch (error) {
    res.status(500).json({ error: 'Failed to vote' });
  }
});

export default router;
