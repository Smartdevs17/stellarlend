import request from 'supertest';
import app from '../app';
import { insuranceService } from '../services/insurance/insurance.service';
import { generateToken } from '../middleware/auth';

const ADMIN_USER = 'GAADMINUSERADDRESSFORTESTING12345678901234567890123456789';
const PROVIDER_USER = 'GAPROVIDERUSERADDRESS123456789012345678901234567890123456';
const LENDER_USER = 'GALENDERUSERADDRESS12345678901234567890123456789012345678';
const ATTACKER_USER = 'GAATTACKERUSERADDRESS123456789012345678901234567890123456';

describe('Insurance routes authentication and security invariants', () => {
  let adminToken: string;
  let providerToken: string;
  let lenderToken: string;
  let attackerToken: string;

  beforeEach(() => {
    insuranceService.resetStore();
    adminToken = generateToken(ADMIN_USER);
    providerToken = generateToken(PROVIDER_USER);
    lenderToken = generateToken(LENDER_USER);
    attackerToken = generateToken(ATTACKER_USER);
  });

  it('allows public read and calculator routes without authentication', async () => {
    const policiesRes = await request(app).get('/api/insurance/policies');
    expect(policiesRes.status).toBe(200);

    const providersRes = await request(app).get('/api/insurance/providers');
    expect(providersRes.status).toBe(200);

    const analyticsRes = await request(app).get('/api/insurance/analytics');
    expect(analyticsRes.status).toBe(200);
  });

  it('rejects unauthenticated provider onboarding, policy creation, coverage purchase, and claim submission', async () => {
    const unauthProvider = await request(app).post('/api/insurance/providers').send({
      name: 'Rogue Insurer',
      collateral: 100_000,
    });
    expect(unauthProvider.status).toBe(401);

    const unauthPolicy = await request(app).post('/api/insurance/policies').send({
      providerId: 'some-id',
      coverageAmount: 10_000,
      premiumBps: 200,
      durationDays: 30,
      terms: 'Terms',
      coveredTriggers: ['oracle_failure'],
      exclusions: [],
    });
    expect(unauthPolicy.status).toBe(401);

    const unauthPurchase = await request(app).post('/api/insurance/coverages').send({
      policyId: 'some-id',
      positionId: 'pos-1',
      coverageAmount: 5_000,
    });
    expect(unauthPurchase.status).toBe(401);

    const unauthClaim = await request(app).post('/api/insurance/claims').send({
      coverageId: 'cov-1',
      trigger: 'oracle_failure',
      evidence: 'evidence',
      amount: 1_000,
    });
    expect(unauthClaim.status).toBe(401);
  });

  it('forces provider kycStatus to pending upon onboarding and requires admin role to approve KYC', async () => {
    // 1. Provider onboards, attempting to self-approve KYC
    const onboardRes = await request(app)
      .post('/api/insurance/providers')
      .set('Authorization', `Bearer ${providerToken}`)
      .send({
        name: 'Nexus Mutual Clone',
        kycStatus: 'approved', // Attempting bypass
        collateral: 500_000,
      });
    expect(onboardRes.status).toBe(201);
    expect(onboardRes.body.data.kycStatus).toBe('pending');
    expect(onboardRes.body.data.address).toBe(PROVIDER_USER);
    const providerId = onboardRes.body.data.id;

    // 2. Policy creation must be rejected while KYC is pending
    const policyWhilePending = await request(app)
      .post('/api/insurance/policies')
      .set('Authorization', `Bearer ${providerToken}`)
      .send({
        providerId,
        coverageAmount: 100_000,
        premiumBps: 150,
        durationDays: 60,
        terms: 'Oracle failure cover',
        coveredTriggers: ['oracle_failure'],
        exclusions: [],
      });
    expect(policyWhilePending.status).toBe(400);
    expect(policyWhilePending.body.error).toMatch(/Approved provider required/);

    // 3. Non-admin attempting to approve KYC is rejected
    const nonAdminKyc = await request(app)
      .patch(`/api/insurance/providers/${providerId}/kyc`)
      .set('Authorization', `Bearer ${providerToken}`)
      .set('x-user-role', 'user')
      .set('x-user-address', PROVIDER_USER)
      .send({ status: 'approved' });
    expect(nonAdminKyc.status).toBe(401);

    // 4. Admin successfully approves KYC
    const adminKyc = await request(app)
      .patch(`/api/insurance/providers/${providerId}/kyc`)
      .set('Authorization', `Bearer ${adminToken}`)
      .set('x-user-role', 'admin')
      .set('x-user-address', ADMIN_USER)
      .send({ status: 'approved' });
    expect(adminKyc.status).toBe(200);
    expect(adminKyc.body.data.kycStatus).toBe('approved');

    // 5. Provider can now create policy
    const policyRes = await request(app)
      .post('/api/insurance/policies')
      .set('Authorization', `Bearer ${providerToken}`)
      .send({
        providerId,
        coverageAmount: 100_000,
        premiumBps: 150,
        durationDays: 60,
        terms: 'Oracle failure cover',
        coveredTriggers: ['oracle_failure'],
        exclusions: [],
      });
    expect(policyRes.status).toBe(201);
    expect(policyRes.body.data.active).toBe(true);
  });

  it('prevents cross-user claim manipulation and binds coverage to authenticated lender', async () => {
    // Setup approved provider and policy
    const provider = insuranceService.onboardProvider({
      address: PROVIDER_USER,
      name: 'SafeInsure',
      kycStatus: 'approved',
      collateral: 200_000,
    });
    const policy = insuranceService.createPolicy({
      providerId: provider.id,
      coverageAmount: 50_000,
      premiumBps: 200,
      durationDays: 30,
      terms: 'Coverage terms',
      coveredTriggers: ['oracle_failure'],
      exclusions: [],
    });

    // Lender purchases coverage
    const purchaseRes = await request(app)
      .post('/api/insurance/coverages')
      .set('Authorization', `Bearer ${lenderToken}`)
      .send({
        policyId: policy.id,
        positionId: 'lender-pos-99',
        coverageAmount: 20_000,
      });
    expect(purchaseRes.status).toBe(201);
    expect(purchaseRes.body.data.lender).toBe(LENDER_USER);
    const coverageId = purchaseRes.body.data.id;

    // Attacker attempts to submit claim on lender's coverage
    const attackerClaim = await request(app)
      .post('/api/insurance/claims')
      .set('Authorization', `Bearer ${attackerToken}`)
      .send({
        coverageId,
        trigger: 'oracle_failure',
        evidence: 'fake-evidence',
        amount: 5_000,
      });
    expect(attackerClaim.status).toBe(401);
    expect(attackerClaim.body.error).toMatch(/does not own this coverage/);

    // Legitimate lender submits claim successfully
    const lenderClaim = await request(app)
      .post('/api/insurance/claims')
      .set('Authorization', `Bearer ${lenderToken}`)
      .send({
        coverageId,
        trigger: 'oracle_failure',
        evidence: 'valid oracle failure tx proof',
        amount: 5_000,
      });
    expect(lenderClaim.status).toBe(201);
    expect(lenderClaim.body.data.amount).toBe(5_000);
  });
});
