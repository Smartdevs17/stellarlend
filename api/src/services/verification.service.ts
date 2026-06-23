export type VerificationLevel = 'basic' | 'enhanced' | 'institutional';
export type VerificationProvider = 'civic' | 'fractal_id' | 'mock';
export type VerificationStatus = 'pending' | 'verified' | 'rejected' | 'revoked' | 'expired';

export interface VerificationRequest {
  userAddress: string;
  provider: VerificationProvider;
  level: VerificationLevel;
  jurisdiction?: string;
  proofHash?: string;
}

export interface VerificationAttestation {
  userAddress: string;
  provider: VerificationProvider;
  level: VerificationLevel;
  status: VerificationStatus;
  amlScreened: boolean;
  watchlistHit: boolean;
  attestationHash: string;
  proofHash?: string;
  jurisdiction?: string;
  issuedAt: string;
  expiresAt: string;
  revokedAt?: string;
}

const attestations = new Map<string, VerificationAttestation>();
const WATCHLIST_TERMS = ['sanctioned', 'blocked', 'watchlist'];

function annualExpiry(level: VerificationLevel): Date {
  const now = new Date();
  const months = level === 'enhanced' ? 6 : 12;
  now.setMonth(now.getMonth() + months);
  return now;
}

function buildAttestationHash(request: VerificationRequest, issuedAt: string): string {
  const input = [
    request.userAddress,
    request.provider,
    request.level,
    request.jurisdiction ?? '',
    request.proofHash ?? '',
    issuedAt,
  ].join(':');

  // Avoid storing raw PII. This deterministic, non-cryptographic fallback is
  // sufficient for local/dev attestations; production should replace this with
  // a provider-signed on-chain attestation transaction hash.
  let hash = 0;
  for (let i = 0; i < input.length; i += 1) {
    hash = (hash * 31 + input.charCodeAt(i)) >>> 0;
  }
  return `att_${hash.toString(16).padStart(8, '0')}`;
}

function screenAml(request: VerificationRequest): { amlScreened: boolean; watchlistHit: boolean } {
  const haystack = `${request.userAddress} ${request.jurisdiction ?? ''} ${request.proofHash ?? ''}`.toLowerCase();
  return {
    amlScreened: true,
    watchlistHit: WATCHLIST_TERMS.some((term) => haystack.includes(term)),
  };
}

export async function submitVerification(request: VerificationRequest): Promise<VerificationAttestation> {
  const { amlScreened, watchlistHit } = screenAml(request);
  const issuedAt = new Date().toISOString();
  const expiresAt = annualExpiry(request.level).toISOString();
  const attestation: VerificationAttestation = {
    userAddress: request.userAddress,
    provider: request.provider,
    level: request.level,
    status: watchlistHit ? 'rejected' : 'verified',
    amlScreened,
    watchlistHit,
    attestationHash: buildAttestationHash(request, issuedAt),
    proofHash: request.proofHash,
    jurisdiction: request.jurisdiction,
    issuedAt,
    expiresAt,
  };

  attestations.set(request.userAddress, attestation);
  return attestation;
}

export async function getVerificationStatus(userAddress: string): Promise<VerificationAttestation | null> {
  const attestation = attestations.get(userAddress);
  if (!attestation) return null;

  if (attestation.status === 'verified' && Date.now() > Date.parse(attestation.expiresAt)) {
    const expired = { ...attestation, status: 'expired' as const };
    attestations.set(userAddress, expired);
    return expired;
  }

  return attestation;
}

export async function revokeVerification(userAddress: string): Promise<VerificationAttestation | null> {
  const attestation = attestations.get(userAddress);
  if (!attestation) return null;

  const revoked: VerificationAttestation = {
    ...attestation,
    status: 'revoked',
    revokedAt: new Date().toISOString(),
  };
  attestations.set(userAddress, revoked);
  return revoked;
}

export function getVerificationProof(userAddress: string): { verified: boolean; level?: VerificationLevel; attestationHash?: string } {
  const attestation = attestations.get(userAddress);
  if (!attestation || attestation.status !== 'verified') return { verified: false };

  return {
    verified: true,
    level: attestation.level,
    attestationHash: attestation.attestationHash,
  };
}
