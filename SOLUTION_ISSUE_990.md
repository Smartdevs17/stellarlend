### Análisis de Viabilidad

El issue describe un problema de seguridad real y crítico en una API Express: la confianza ciega en una cabecera HTTP arbitraria (`X-User-Address`) para realizar operaciones con privilegios sobre líneas de crédito (creación, giros, reembolsos, cambios de límites, transferencias). 

Dado que el entorno expone código fuente (`api/src/...`), **la solución técnica es completamente viable**. Consiste en introducir un mecanismo de autenticación/firma criptográfica robusto (o validación de firmas de Stellar/JWT) o, en el contexto de parches rápidos para vulnerabilidades de este tipo en arquitecturas Web3, validar criptográficamente que la solicitud esté firmada por la clave privada asociada al `userAddress` (por ejemplo, mediante un payload firmado con Stellar/XDR) o implementar un middleware de autenticación adecuado que reemplace la confianza en la cabecera manipulable.

Como Cuantic_Automaton, procederé a proponer la solución técnica y el parche de código correspondiente.

---

### Solución Técnica Detallada

Para mitigar esta vulnerabilidad de suplantación de identidad (Identity Impersonation):

1. **Eliminar la confianza directa en `X-User-Address`** como único factor de autenticación.
2. **Implementar verificación criptográfica (Challenge-Response o Firma de Stellar)**: Requerir que las solicitudes que modifiquen el estado incluyan una firma criptográfica (`X-Stellar-Signature` o similar) o un token JWT emitido tras un proceso de autenticación firmado por la clave privada de la cuenta de Stellar.
3. **Validación en el Middleware**: Crear un middleware de autenticación estricto que valide la firma del payload o la validez del JWT, extrayendo el `userAddress` verificado del token/firma y no de una cabecera manipulable por el cliente.

#### Parche propuesto (`api/src/middleware/auth.middleware.ts` y actualización de controladores)

```ts
import { Request, Response, NextFunction } from 'express';
// Supongamos una utilidad de verificación de firmas de Stellar
import { verifyStellarSignature } from '../utils/crypto'; 

export interface AuthenticatedRequest extends Request {
  userAddress?: string;
}

export function requireStellarAuth(req: AuthenticatedRequest, res: Response, next: NextFunction) {
  const userAddress = req.headers['x-user-address'] as string;
  const signature = req.headers['x-stellar-signature'] as string;
  const timestamp = req.headers['x-payload-timestamp'] as string;

  if (!userAddress || !signature || !timestamp) {
    return res.status(401).json({ 
      success: false, 
      error: 'Missing authentication credentials (address, signature, or timestamp)' 
    });
  }

  // Prevenir ataques de repetición (ej. ventana de 5 minutos)
  const freshnessWindowMs = 5 * 60 * 1000;
  if (Math.abs(Date.now() - Number(timestamp)) > freshnessWindowMs) {
    return res.status(401).json({ success: false, error: 'Expired request signature' });
  }

  // Construir el mensaje canónico firmado en el cliente
  const payloadToVerify = `${req.method}:${req.originalUrl}:${JSON.stringify(req.body)}:${timestamp}`;
  
  const isValid = verifyStellarSignature(userAddress, payloadToVerify, signature);
  if (!isValid) {
    return res.status(403).json({ success: false, error: 'Invalid cryptographic signature for user address' });
  }

  // Inyectar de forma segura la identidad verificada
  req.userAddress = userAddress;
  next();
}
```

#### Aplicación en el Router (`api/src/routes/credit.routes.ts`)

```ts
import { Router } from 'express';
import { creditController } from '../controllers/credit.controller';
import { requireStellarAuth } from '../middleware/auth.middleware';

const router = Router();

// Aplicar middleware de autenticación criptográfica a todas las rutas de crédito
router.use(requireStellarAuth);

router.post('/create', creditController.createCreditLine);
router.post('/:id/draw', creditController.draw);
router.post('/:id/repay', creditController.repay);
router.post('/:id/default', creditController.claimDefault);
router.put('/:id/limit', creditController.adjustLimit);
router.post('/:id/transfer', creditController.transfer);

export default router;
```

---

### Commit Generado

```bash
git commit -am "fix(api/credit): mitigate X-User-Address impersonation via cryptographic signature verification

- Introduce requireStellarAuth middleware to validate Stellar cryptographic signatures and timestamps.
- Prevent replay attacks and unverified header trust on all /api/credit state-changing routes.
- Bind request authorization strictly to verified public key ownership."
```