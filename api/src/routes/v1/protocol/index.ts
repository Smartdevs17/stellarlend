/**
 * Protocol Domain Routes (v1)
 *
 * Aggregates all protocol-level routes under /v1/protocol:
 * - Protocol statistics and status
 * - Pause/resume controls
 * - Role-based access management
 * - Audit logging and integrity verification
 */

import protocolFeatureRoutes from '../../features/protocol.module';

export default protocolFeatureRoutes;
