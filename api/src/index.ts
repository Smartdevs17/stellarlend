import { config } from './config';
import app from './app';

const server = app.listen(config.port, () => {
  console.log(`Server running on port ${config.port} in ${config.nodeEnv} mode`);
  console.log(`Connected to Stellar network: ${config.stellarNetwork}`);
  console.log(`Using contract: ${config.contractId}`);
});

process.on('unhandledRejection', (err: Error) => {
  console.error('Unhandled Rejection:', err.message);
  server.close(() => process.exit(1));
});

process.on('uncaughtException', (err: Error) => {
  console.error('Uncaught Exception:', err.message);
  server.close(() => process.exit(1));
});

export default server;
