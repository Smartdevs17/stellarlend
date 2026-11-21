/**
 * Lending Domain Routes (v1)
 *
 * Aggregates all lending-related routes under /v1/lending:
 * - Core lending operations (prepare, submit, transactions)
 * - Gas estimation
 * - Debt token operations
 * - Cross-asset operations
 */

import lendingFeatureRoutes from '../../features/lending.module';

export default lendingFeatureRoutes;
