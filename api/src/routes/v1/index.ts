/**
 * V1 API Router
 *
 * Aggregates all v1 domain routers under versioned prefixes.
 * Route structure:
 *   /api/v1/lending     -> lending domain
 *   /api/v1/protocol    -> protocol domain
 *   /api/v1/governance  -> governance domain
 *   /api/v1/account     -> user account domain
 *   /api/v1/system      -> infrastructure domain
 *   /api/v1/security    -> security/privacy domain
 */

import { Router } from 'express';
import lendingV1Routes from './lending';
import protocolV1Routes from './protocol';
import governanceV1Routes from './governance';
import oracleV1Routes from './oracle';
import accountV1Routes from './account';
import systemV1Routes from './system';
import securityV1Routes from './security';

const router = Router();

router.use('/lending', lendingV1Routes);
router.use('/protocol', protocolV1Routes);
router.use('/governance', governanceV1Routes);
router.use('/oracle', oracleV1Routes);
router.use('/account', accountV1Routes);
router.use('/system', systemV1Routes);
router.use('/security', securityV1Routes);

export default router;
