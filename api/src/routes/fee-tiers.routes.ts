import { Router } from 'express';
import { feeTierService } from '../services/fees/tier.service';

const router = Router();

// Get all configured fee tiers
router.get('/tiers', (_req, res) => {
  res.json({ success: true, data: feeTierService.getTiers() });
});

// Configure fee tiers (admin)
router.put('/config', (req, res) => {
  try {
    res.json({ success: true, data: feeTierService.configure(req.body.tiers) });
  } catch (error) {
    res
      .status(400)
      .json({ success: false, error: error instanceof Error ? error.message : 'Invalid tiers' });
  }
});

// Get user fee tier status and loyalty progress
router.post('/status', (req, res) =>
  res.json({ success: true, data: feeTierService.status(req.body.userAddress, req.body.metrics) })
);

// Apply fee tier discount to a transaction
router.post('/apply', (req, res) =>
  res.json({
    success: true,
    data: feeTierService.apply(
      req.body.userAddress,
      Number(req.body.baseFee),
      req.body.metrics,
      Number(req.body.minimumFee ?? 0)
    ),
  })
);

// Fee transparency: detailed itemized calculation breakdown
router.post('/transparency', (req, res) => {
  const { userAddress, operation, amount, baseFeePercent, metrics } = req.body;
  if (!userAddress || !operation || amount === undefined || baseFeePercent === undefined || !metrics) {
    return res.status(400).json({
      success: false,
      error: 'userAddress, operation, amount, baseFeePercent, and metrics are required',
    });
  }
  const breakdown = feeTierService.getTransparency(
    userAddress,
    operation,
    Number(amount),
    Number(baseFeePercent),
    metrics
  );
  return res.json({ success: true, data: breakdown });
});

// Fee analytics: aggregate discount and tier distribution statistics
router.get('/analytics', (_req, res) => {
  res.json({ success: true, data: feeTierService.getAnalytics() });
});

export default router;
