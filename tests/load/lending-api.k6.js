import http from 'k6/http';
import { check, sleep } from 'k6';

const baseUrl = __ENV.LENDING_API_BASE_URL || 'http://localhost:3000';
const user = `load-user-${__VU}`;

export const options = {
  scenarios: {
    lending_api: {
      executor: 'constant-arrival-rate',
      rate: Number(__ENV.K6_RATE || 20),
      timeUnit: '1s',
      duration: __ENV.K6_DURATION || '30s',
      preAllocatedVUs: 10,
      maxVUs: 50,
    },
  },
  thresholds: {
    http_req_failed: ['rate<0.01'],
    http_req_duration: ['p(95)<500', 'p(99)<1000'],
  },
};

export default function () {
  const position = http.get(`${baseUrl}/api/positions/${user}`);
  check(position, { 'position returns success': (response) => response.status === 200 });

  const health = http.get(`${baseUrl}/api/health`);
  check(health, { 'health returns success': (response) => response.status === 200 });
  sleep(0.1);
}
