import { Router } from 'express';
import eventsRoutes from '../events';

const router = Router();
router.use('/', eventsRoutes);

export default router;
