import { Router } from 'express';
import { sandboxService } from '../services/sandbox.service';

const router: Router = Router();

/**
 * @openapi
 * /sandbox:
 *   get:
 *     summary: Get sandbox environment status
 *     tags: [Sandbox]
 *     responses:
 *       200:
 *         description: Sandbox enabled flag, isolation model, and resource counts
 */
router.get('/', (_req, res) => {
  res.json({ success: true, data: sandboxService.status() });
});

/**
 * @openapi
 * /sandbox/reset:
 *   post:
 *     summary: Reset sandbox state and re-seed a read-only demo key
 *     tags: [Sandbox]
 *     responses:
 *       200:
 *         description: Sandbox reset. The seeded raw key is returned once.
 *       403:
 *         description: Sandbox is disabled in this environment
 */
router.post('/reset', async (_req, res, next) => {
  try {
    const result = await sandboxService.reset();
    res.json({ success: true, data: result });
  } catch (error: any) {
    if (String(error?.message).includes('disabled')) {
      res.status(403).json({ success: false, error: error.message });
      return;
    }
    next(error);
  }
});

export default router;
