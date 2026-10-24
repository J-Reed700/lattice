import type {
  FinderStatusDto,
  KeyServiceDto,
  KeyStatusDto,
  ReachGroupDto,
} from '@/lib/bindings';

export const REACH_LABELS: Record<ReachGroupDto, string> = {
  openWeb: 'Web search engines',
  ownerSites: 'Official sites',
  registry: 'Package registries',
  codeHost: 'Code hosts',
  scholarly: 'Scholarly',
  standards: 'Standards',
  law: 'Law and government',
  community: 'Community',
  encyclopedia: 'Encyclopedias',
  archive: 'Internet Archive',
  linkedPages: 'Links on a page',
};

export type Tone = 'muted' | 'ok' | 'warning';

/** The key service a finder uses, when it takes a key. */
export function keyServiceOf(finder: FinderStatusDto): KeyServiceDto | null {
  return finder.needs.kind === 'optionalKey' || finder.needs.kind === 'requiredKey'
    ? finder.needs.service
    : null;
}

function keyName(service: KeyServiceDto, keys: KeyStatusDto[]): string {
  return keys.find((key) => key.service === service)?.name ?? service;
}

function clock(iso: string): string {
  const date = new Date(iso);
  return Number.isNaN(date.getTime())
    ? iso
    : date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

/** One line on what a finder is doing: why it is off, or how it is faring. */
export function statusLine(
  finder: FinderStatusDto,
  keys: KeyStatusDto[],
): { text: string; tone: Tone } {
  switch (finder.gate) {
    case 'disabled':
      return { text: 'Off', tone: 'muted' };
    case 'noKey': {
      const service = keyServiceOf(finder);
      return {
        text: service ? `Needs a ${keyName(service, keys)} key` : 'Needs a key',
        tone: 'muted',
      };
    }
    case 'noInstance':
      return { text: 'Needs your SearXNG address', tone: 'muted' };
    case 'noContact':
      return { text: 'Needs a contact', tone: 'muted' };
    default:
      break;
  }
  const { health } = finder;
  if (health.state === 'coolingDown') {
    const until = health.until ? ` until ${clock(health.until)}` : '';
    const cause = health.lastError ? ` (${health.lastError})` : '';
    return { text: `Paused${until}${cause}`, tone: 'warning' };
  }
  if (health.state === 'retrying') {
    return { text: 'Trying again after a pause', tone: 'warning' };
  }
  if (health.calls === 0) return { text: 'Ready', tone: 'ok' };
  const median = health.medianMs === null ? '' : ` · median ${(health.medianMs / 1000).toFixed(1)} s`;
  const lastError = health.lastError ? ` · last error: ${health.lastError}` : '';
  const calls = health.calls === 1 ? '1 call' : `${health.calls} calls`;
  return { text: `Healthy · ${calls}${median}${lastError}`, tone: 'ok' };
}

/** An http(s) address with a host, as the backend requires of SearXNG. */
export function isHttpUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return (url.protocol === 'http:' || url.protocol === 'https:') && url.hostname !== '';
  } catch {
    return false;
  }
}

/** An email address or an http(s) address, as the backend requires of a contact. */
export function isContact(value: string): boolean {
  if (value === '' || /\s/.test(value)) return false;
  const at = value.indexOf('@');
  if (at >= 0) {
    const local = value.slice(0, at);
    const domain = value.slice(at + 1);
    return (
      local !== '' &&
      !domain.includes('@') &&
      domain.includes('.') &&
      domain.split('.').every((label) => label !== '')
    );
  }
  return isHttpUrl(value);
}
