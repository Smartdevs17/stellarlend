const requiredEnvVars = [
  'CONTRACT_ID',
  'JWT_SECRET',
  'STELLAR_NETWORK',
  'SOROBAN_RPC_URL'
];

const missing = requiredEnvVars.filter(v => !process.env[v]);

if (missing.length > 0) {
  console.error('Missing required environment variables:', missing.join(', '));
  process.exit(1);
}

export const config = {
  contractId: process.env.CONTRACT_ID!,
  jwtSecret: process.env.JWT_SECRET!,
  stellarNetwork: process.env.STELLAR_NETWORK!,
  sorobanRpcUrl: process.env.SOROBAN_RPC_URL!,
  port: parseInt(process.env.PORT || '3000', 10),
  nodeEnv: process.env.NODE_ENV || 'development'
};
