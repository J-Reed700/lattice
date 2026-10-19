/**
 * Settings → Sources (source-finding toolbelt design §3.14, §3.17): the paid
 * search engines and their keys, the user's own SearXNG server, the contact
 * for polite pools, the keys that raise rate limits, and every other finder
 * grouped by what it reaches, each with its privacy line, switch and health.
 *
 * Switches, the SearXNG address, the contact and the archive toggle are the
 * `sourcing` settings category; keys go to the system keychain through their
 * own commands and never come back: the panel learns only that one is saved.
 * Either way the backend re-registers its finders at once, so a change holds
 * from the next search.
 */

import { useState, type ReactNode } from 'react';

import { PageHeader, SettingsRow, SettingsSection, settingsFieldClass, Switch } from '@/components/ui';
import type {
  FinderStatusDto,
  JsonValue,
  KeyStatusDto,
  ReachGroupDto,
  SourcingSettingsDto,
} from '@/lib/bindings';
import { cn } from '@/lib/utils';
import { toast } from '@/stores/toastStore';
import { openExternalUrl } from '@/utils/openExternalUrl';

import { GHOST_BUTTON_CLASS, SECONDARY_BUTTON_CLASS, SWITCH_CLASS } from './settingsStyles';
import { isContact, isHttpUrl, keyServiceOf, REACH_LABELS, statusLine, type Tone } from './sourcesLabels';
import { useSettingsQuery, useUpdateSettingsMutation } from '../hooks/useSettingsQuery';
import {
  useRefreshSourcesStatus,
  useSourceKeyMutations,
  useSourcesStatus,
} from '../hooks/useSourcesStatus';

/** The backend's defaults, shown until the settings load. */
const DEFAULT_SOURCING: SourcingSettingsDto = {
  finders: {},
  searxngUrl: null,
  contact: null,
  archiveFallback: true,
  finderConcurrency: 4,
};

const TONE_CLASS: Record<Tone, string> = {
  muted: 'text-text-muted',
  ok: 'text-success-fg',
  warning: 'text-warning-fg',
};

const isPaidEngine = (finder: FinderStatusDto) =>
  finder.reach === 'openWeb' && finder.needs.kind === 'requiredKey';

export function SourcesTab() {
  const { data: settings, isLoading } = useSettingsQuery();
  const { data: status, error: statusError } = useSourcesStatus();
  const updateMutation = useUpdateSettingsMutation();
  const refreshStatus = useRefreshSourcesStatus();
  const sourcing = settings?.sourcing ?? DEFAULT_SOURCING;
  const busy = isLoading || updateMutation.isPending;

  const updateSourcing = (updates: Record<string, JsonValue>, failure: string, onDone?: () => void) => {
    updateMutation.mutate(
      { category: 'sourcing', updates },
      {
        onSuccess: () => {
          onDone?.();
          void refreshStatus();
        },
        onError: (error) => toast.error(failure, { message: error.message }),
      }
    );
  };
  const toggleFinder = (finder: FinderStatusDto, enabled: boolean) =>
    updateSourcing({ finders: { [finder.id]: { enabled } } }, `Couldn't switch ${finder.name}`);

  const finders = status?.finders ?? [];
  const keys = status?.keys ?? [];
  const paidEngines = finders.filter(isPaidEngine);
  const searxng = finders.filter((finder) => finder.needs.kind === 'instanceUrl');
  const paidServices = new Set(paidEngines.map(keyServiceOf));
  const otherKeys = keys.filter((key) => !paidServices.has(key.service));
  const shownAbove = new Set([...paidEngines, ...searxng].map((finder) => finder.id));
  const groups = groupByReach(finders.filter((finder) => !shownAbove.has(finder.id)));

  const finderRow = (finder: FinderStatusDto) => (
    <FinderRow key={finder.id} finder={finder} keys={keys} disabled={busy} onToggle={toggleFinder} />
  );

  return (
    <>
      <PageHeader
        title="Sources"
        description="Where Lattice looks for sources on the web, and what each service is sent."
      />

      {statusError ? (
        <p role="alert" className="mb-6 text-sm text-danger">
          Couldn&apos;t read the sources: {statusError.message}
        </p>
      ) : null}

      <SettingsSection
        title="Paid search engines"
        description="Off until you add a key. Each runs under your own account and plan."
      >
        {paidEngines.map((finder) => {
          const keyStatus = keys.find((key) => key.service === keyServiceOf(finder));
          return (
            <SettingsRow
              key={finder.id}
              label={finder.name}
              hint={<FinderHint finder={finder} keys={keys} />}
              stacked
            >
              <div className="flex flex-wrap items-center gap-3">
                {keyStatus ? <KeyField keyStatus={keyStatus} /> : null}
                <FinderSwitch finder={finder} disabled={busy} onToggle={toggleFinder} />
              </div>
            </SettingsRow>
          );
        })}
      </SettingsSection>

      <SettingsSection title="Your SearXNG server">
        <AddressField
          id="searxngUrl"
          label="Server address"
          hint="Your own instance, on this computer, your network or the web, with its JSON output turned on. Lattice sends it nothing until you set one, and asks that address for nothing but searches."
          saved={sourcing.searxngUrl}
          placeholder="https://search.example.org"
          check={isHttpUrl}
          invalidMessage="Enter an address that starts with http:// or https://"
          disabled={busy}
          onSave={(searxngUrl, done) =>
            updateSourcing({ searxngUrl }, "Couldn't save the SearXNG address", done)
          }
        />
        {searxng.map(finderRow)}
      </SettingsSection>

      <SettingsSection title="Contact for polite pools">
        <AddressField
          id="sourcingContact"
          label="Your email or web address"
          hint={
            <>
              Crossref and Unpaywall ask callers to say who they are and give them a faster,
              politer pool. Your contact is sent to them with each lookup, and added to the
              agent string Lattice sends the other services it asks. Off by default: without it,
              Unpaywall stays off and Crossref is asked anonymously.
            </>
          }
          saved={sourcing.contact}
          placeholder="you@example.org"
          check={isContact}
          invalidMessage="Enter an email address, or a web address that starts with http:// or https://"
          disabled={busy}
          onSave={(contact, done) => updateSourcing({ contact }, "Couldn't save the contact", done)}
        />
      </SettingsSection>

      <SettingsSection
        title="Keys that raise rate limits"
        description="Optional. Each key stays in your system keychain."
      >
        {otherKeys.map((keyStatus) => (
          <SettingsRow
            key={keyStatus.service}
            label={keyStatus.name}
            hint={keyUse(keyStatus, finders)}
            stacked
          >
            <KeyField keyStatus={keyStatus} />
          </SettingsRow>
        ))}
      </SettingsSection>

      {groups.map(([reach, members]) => (
        <SettingsSection key={reach} title={REACH_LABELS[reach]}>
          {reach === 'archive' ? (
            <SettingsRow
              label="Read pages from the Internet Archive"
              hint="When a page will not load, ask archive.org for a saved copy; also allows dated reads. Sends the page's address to the Internet Archive."
            >
              <Switch
                className={SWITCH_CLASS}
                checked={sourcing.archiveFallback}
                disabled={busy}
                onCheckedChange={(archiveFallback) =>
                  updateSourcing({ archiveFallback }, "Couldn't switch the archive fallback")
                }
                aria-label="Read pages from the Internet Archive"
              />
            </SettingsRow>
          ) : null}
          {members.map(finderRow)}
        </SettingsSection>
      ))}
    </>
  );
}

/** Finders by what they reach, groups in the order the backend lists them. */
function groupByReach(finders: FinderStatusDto[]): [ReachGroupDto, FinderStatusDto[]][] {
  const groups = new Map<ReachGroupDto, FinderStatusDto[]>();
  for (const finder of finders) {
    const members = groups.get(finder.reach) ?? [];
    members.push(finder);
    groups.set(finder.reach, members);
  }
  return [...groups.entries()];
}

/** What a key does for the finders that use it. */
function keyUse(keyStatus: KeyStatusDto, finders: FinderStatusDto[]): string {
  return finders
    .filter((finder) => keyServiceOf(finder) === keyStatus.service)
    .map((finder) =>
      finder.needs.kind === 'requiredKey'
        ? `${finder.name} runs only with this key.`
        : `${finder.name} works without it; a key raises its limits.`
    )
    .join(' ');
}

function FinderHint({ finder, keys }: { finder: FinderStatusDto; keys: KeyStatusDto[] }) {
  const line = statusLine(finder, keys);
  return (
    <>
      <span className="block">{finder.disclosure}.</span>
      <span className={cn('block', TONE_CLASS[line.tone])} data-testid={`status-${finder.id}`}>
        {line.text}
      </span>
    </>
  );
}

function FinderSwitch({
  finder,
  disabled,
  onToggle,
}: {
  finder: FinderStatusDto;
  disabled: boolean;
  onToggle: (finder: FinderStatusDto, enabled: boolean) => void;
}) {
  return (
    <Switch
      className={SWITCH_CLASS}
      checked={finder.enabled}
      disabled={disabled}
      onCheckedChange={(enabled) => onToggle(finder, enabled)}
      aria-label={`Use ${finder.name}`}
    />
  );
}

function FinderRow({
  finder,
  keys,
  disabled,
  onToggle,
}: {
  finder: FinderStatusDto;
  keys: KeyStatusDto[];
  disabled: boolean;
  onToggle: (finder: FinderStatusDto, enabled: boolean) => void;
}) {
  return (
    <SettingsRow label={finder.name} hint={<FinderHint finder={finder} keys={keys} />}>
      <FinderSwitch finder={finder} disabled={disabled} onToggle={onToggle} />
    </SettingsRow>
  );
}

/** A key's password field: Save stores it, Remove deletes it; it is never shown back. */
function KeyField({ keyStatus }: { keyStatus: KeyStatusDto }) {
  const [draft, setDraft] = useState('');
  const { saveKey, removeKey } = useSourceKeyMutations();
  const working = saveKey.isPending || removeKey.isPending;
  const id = `source-key-${keyStatus.service}`;

  const save = () =>
    saveKey.mutate(
      { service: keyStatus.service, key: draft.trim() },
      {
        onSuccess: () => {
          setDraft('');
          toast.success(`${keyStatus.name} key saved`);
        },
        onError: (error) => toast.error(`Couldn't save the ${keyStatus.name} key`, { message: error.message }),
      }
    );
  const remove = () =>
    removeKey.mutate(keyStatus.service, {
      onSuccess: () => toast.success(`${keyStatus.name} key removed`),
      onError: (error) => toast.error(`Couldn't remove the ${keyStatus.name} key`, { message: error.message }),
    });

  return (
    <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
      <label htmlFor={id} className="sr-only">
        {keyStatus.name} key
      </label>
      <input
        id={id}
        type="password"
        autoComplete="off"
        spellCheck={false}
        className={cn(settingsFieldClass, 'min-w-[14rem] flex-1')}
        value={draft}
        placeholder={keyStatus.stored ? 'Paste a new key to replace it' : 'Paste your key'}
        onChange={(event) => setDraft(event.target.value)}
      />
      <button
        type="button"
        className={SECONDARY_BUTTON_CLASS}
        disabled={working || !draft.trim()}
        onClick={save}
      >
        {saveKey.isPending ? 'Saving…' : 'Save'}
      </button>
      {keyStatus.stored ? (
        <>
          <span className="text-xs text-success-fg">Key saved</span>
          <button type="button" className={GHOST_BUTTON_CLASS} disabled={working} onClick={remove}>
            Remove
          </button>
        </>
      ) : null}
      <button
        type="button"
        className={GHOST_BUTTON_CLASS}
        onClick={() => void openExternalUrl(keyStatus.keyPageUrl)}
      >
        Get a key
      </button>
    </div>
  );
}

/** An address the backend checks too; Save sends it, or clears it when empty. */
function AddressField({
  id,
  label,
  hint,
  saved,
  placeholder,
  check,
  invalidMessage,
  disabled,
  onSave,
}: {
  id: string;
  label: string;
  hint: ReactNode;
  saved: string | null;
  placeholder: string;
  check: (value: string) => boolean;
  invalidMessage: string;
  disabled: boolean;
  onSave: (value: string | null, done: () => void) => void;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const value = draft ?? saved ?? '';
  const trimmed = value.trim();
  const invalid = trimmed !== '' && !check(trimmed);
  const changed = trimmed !== (saved ?? '');
  const messageId = `${id}-error`;

  return (
    <SettingsRow label={label} hint={hint} htmlFor={id} stacked>
      <div className="flex gap-2">
        <input
          id={id}
          type="text"
          spellCheck={false}
          className={settingsFieldClass}
          value={value}
          placeholder={placeholder}
          aria-invalid={invalid}
          aria-describedby={invalid ? messageId : undefined}
          onChange={(event) => setDraft(event.target.value)}
        />
        <button
          type="button"
          className={SECONDARY_BUTTON_CLASS}
          disabled={disabled || invalid || !changed}
          onClick={() => onSave(trimmed === '' ? null : trimmed, () => setDraft(null))}
        >
          {trimmed === '' && saved ? 'Clear' : 'Save'}
        </button>
      </div>
      {invalid ? (
        <p id={messageId} role="alert" className="text-xs text-danger-fg">
          {invalidMessage}
        </p>
      ) : null}
    </SettingsRow>
  );
}
