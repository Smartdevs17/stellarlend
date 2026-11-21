import { Router } from 'express';
import protocolRoutes from '../protocol.routes';

const router = Router();
router.use('/', protocolRoutes);

export default router;
