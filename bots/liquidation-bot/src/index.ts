/**
 * Issue #1600 — entry point
 *
 * When USE_ACTORS=true (the new default) the bot starts through the
 * ActorSupervisor which runs each concern as an independent async actor
 * connected by typed channels.
 *
 * The original LiquidationBotService is kept for backwards-compatibility and
 * can be selected with USE_ACTORS=false.
 */

import { LiquidationBotService } from './liquidationBot.service';
import { ActorSupervisor } from './actors/supervisor';

const useActors = process.env.USE_ACTORS !== 'false';

async function main() {
  if (useActors) {
    const supervisor = new ActorSupervisor();
    await supervisor.start();

    const shutdown = async () => {
      console.log('\nShutting down liquidation bot (actor mode)...');
      await supervisor.stop();
      process.exit(0);
    };

    process.on('SIGINT', () => void shutdown());
    process.on('SIGTERM', () => void shutdown());
  } else {
    const bot = new LiquidationBotService();
    await bot.start();

    const shutdown = () => {
      console.log('\nShutting down liquidation bot...');
      bot.stop();
      process.exit(0);
    };

    process.on('SIGINT', shutdown);
    process.on('SIGTERM', shutdown);
  }

  process.on('unhandledRejection', (reason: unknown) => {
    console.error('Unhandled rejection:', reason);
  });

  process.on('uncaughtException', (error: Error) => {
    console.error('Uncaught exception:', error);
    process.exit(1);
  });
}

void main();
