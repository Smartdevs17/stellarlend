import { URL } from 'url';

export class EgressSecurityError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'EgressSecurityError';
  }
}

export class EgressAllowlist {
  private allowedHostnames: Set<string> = new Set();
  private allowedProtocols: Set<string> = new Set(['https:']);

  constructor(initialHosts: string[] = []) {
    initialHosts.forEach(host => this.allowedHostnames.add(host.toLowerCase()));
  }

  public allowHost(hostname: string): void {
    this.allowedHostnames.add(hostname.toLowerCase());
  }

  public validateUrl(rawUrl: string): URL {
    let parsed: URL;
    try {
      parsed = new URL(rawUrl);
    } catch {
      throw new EgressSecurityError(`Malformed egress URL: ${rawUrl}`);
    }

    if (!this.allowedProtocols.has(parsed.protocol)) {
      throw new EgressSecurityError(`Blocked outbound egress protocol: ${parsed.protocol}`);
    }

    const host = parsed.hostname.toLowerCase();

    // Prevent loopback, private IP ranges and cloud metadata IP accesses (SSRF defense)
    if (
      host === 'localhost' ||
      host === '127.0.0.1' ||
      host === '169.254.169.254' ||
      host.startsWith('10.') ||
      host.startsWith('192.168.') ||
      host.endsWith('.internal')
    ) {
      throw new EgressSecurityError(`Restricted internal address egress blocked: ${host}`);
    }

    const isExplicitlyAllowed = this.allowedHostnames.has(host) ||
      Array.from(this.allowedHostnames).some(allowed => allowed.startsWith('*.') && host.endsWith(allowed.slice(2)));

    if (!isExplicitlyAllowed) {
      throw new EgressSecurityError(`Outbound egress destination not in allowlist: ${host}`);
    }

    return parsed;
  }
}
