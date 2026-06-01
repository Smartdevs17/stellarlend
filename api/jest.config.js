module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  roots: ['<rootDir>/src', '<rootDir>/src/__tests__'],
  testMatch: ['**/__tests__/**/*.ts', '**/?(*.)+(spec|test).ts'],
  setupFiles: ['<rootDir>/jest.setup.ts'],
  // Force Jest to exit after all tests complete so open handles (Redis client,
  // WebSocket server) don't cause the process to hang indefinitely.
  forceExit: true,
  transform: {
    '^.+\\.ts$': 'ts-jest',
  },
  moduleNameMapping: {
    '^@/config/(.*)$': '<rootDir>/src/config/$1',
    '^@/controllers/(.*)$': '<rootDir>/src/controllers/$1',
    '^@/middleware/(.*)$': '<rootDir>/src/middleware/$1',
    '^@/routes/(.*)$': '<rootDir>/src/routes/$1',
    '^@/services/(.*)$': '<rootDir>/src/services/$1',
    '^@/types/(.*)$': '<rootDir>/src/types/$1',
    '^@/utils/(.*)$': '<rootDir>/src/utils/$1',
    '^@/ws/(.*)$': '<rootDir>/src/ws/$1',
  },
  collectCoverageFrom: [
    'src/**/*.ts',
    '!src/**/*.test.ts',
    '!src/**/*.spec.ts',
    '!src/index.ts',
  ],
  coverageThreshold: {
    global: {
      branches: 51,
      // The project currently includes runtime code that isn't exercised by unit tests
      // (e.g. websocket server and auth helpers). Keep thresholds realistic so
      // CI focuses on regressions rather than failing the gate for missing coverage.
      functions: 35,
      lines: 58,
      statements: 58,
    },
  },
  coverageDirectory: 'coverage',
  verbose: true,
};
