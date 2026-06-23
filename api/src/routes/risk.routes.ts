import { Router } from 'express';
import { riskController } from '../controllers/risk.controller';

const router = Router();

router.get('/pool-health/:poolId', (req, res) => riskController.getPoolHealth(req, res));

router.get('/liquidation-heatmap', (req, res) => riskController.getLiquidationHeatmap(req, res));

router.get('/oracle-health', (req, res) => riskController.getOracleHealth(req, res));

router.get('/safety-score', (req, res) => riskController.getProtocolSafetyScore(req, res));

router.get('/metric-trends', (req, res) => riskController.getMetricTrends(req, res));

router.get('/alerts', (req, res) => riskController.getAlerts(req, res));

router.put('/alert-config', (req, res) => riskController.updateAlertConfig(req, res));

router.get('/user/:address/risk-profile', (req, res) => riskController.getUserRiskProfile(req, res));

export default router;
