import type { Page } from '@playwright/test';

import type { commands } from '../../src/lib/bindings';
import type { CommandName } from '../../src/shared/ipc/routes.generated';

/**
 * The one Tauri IPC mock for the browser e2e suite.
 *
 * `installTauriMock` stands in for the webview bridge: callbacks, events and
 * routing. Fixtures answer commands with `mockCommands` (handlers that run in
 * the page) or `serveCommands` (handlers that run in Node, so their state
 * survives a reload). Handlers are keyed by the generated command names and
 * return the generated DTOs, so `tsc -p tsconfig.e2e.json` fails when a
 * fixture names a command or a field the backend no longer has.
 */

type SnakeToCamel<Name extends string> = Name extends `${infer Head}_${infer Tail}`
  ? `${Head}${Capitalize<SnakeToCamel<Tail>>}`
  : Name;
type Generated = typeof commands;
type Returned<C extends CommandName> = SnakeToCamel<C> extends keyof Generated
  ? Awaited<ReturnType<Generated[SnakeToCamel<C>]>>
  : never;
type ResultData<R> = R extends { status: 'ok'; data: infer Data } ? Data : R extends { status: 'error' } ? never : R;

/** What a command resolves with in the renderer. */
export type CommandResult<C extends CommandName> = ResultData<Returned<C>>;

// Every generated command must map to a generated wrapper; a naming mismatch
// would silently leave its results unchecked.
type Unmapped = { [C in CommandName]: SnakeToCamel<C> extends keyof Generated ? never : C }[CommandName];
const everyCommandHasAContract: [Unmapped] extends [never] ? true : Unmapped = true;
void everyCommandHasAContract;

/**
 * A fixture value: it may leave out fields its flow never reads, but every
 * field it names must exist in the generated DTO, with the generated type.
 */
export type Fixture<T> = T extends readonly (infer Item)[]
  ? Fixture<Item>[]
  : T extends object
    ? { [Key in keyof T]?: Fixture<T[Key]> }
    : T;

/** Commands of Tauri's own plugins that the renderer calls. */
type NativeResults = {
  'dialog|open': string | string[] | null;
  'app|identifier': string;
};
type NativeCommand = keyof NativeResults;

export type MockArgs = Record<string, unknown>;
type Reply<T> = Fixture<T> | (null extends T ? undefined | void : never);
type Handler<T, Context extends unknown[]> = (args: MockArgs, ...context: Context) => Reply<T> | Promise<Reply<T>>;
type Handlers<Context extends unknown[]> = {
  [C in CommandName]?: Handler<CommandResult<C>, Context>;
} & {
  [N in NativeCommand]?: Handler<NativeResults[N], Context>;
};

/** Handlers that run in the page, with the bridge to the renderer. */
export type MockHandlers = Handlers<[ipc: MockIpc]>;
/** Handlers that run in Node. */
export type ServedHandlers = Handlers<[]>;

// TypeScript does not reject unknown fields in an object returned from a
// function, so `Checked` walks what each handler returns against the
// generated result and turns a field the DTO lacks into a type error naming
// its path.
type KeysOf<T> = T extends unknown ? keyof T : never;
type FieldOf<T, Key> = T extends unknown ? (Key extends keyof T ? T[Key] : never) : never;
type ElementOf<T> = T extends readonly (infer Item)[] ? Item : never;
type UnknownFields<Actual, Expected, Path extends string = '', Depth extends unknown[] = []> = Depth['length'] extends 8
  ? never // deep enough for every fixture; recursive DTOs would never end
  : 0 extends 1 & Actual
    ? never // `any`, while a handler's parameters are still being inferred
    : [Actual] extends [readonly (infer Item)[]]
      ? UnknownFields<Item, ElementOf<Expected>, Path, Depth>
      : [Actual] extends [object]
        ? ObjectUnknownFields<Actual, Extract<Expected, object>, Path, Depth>
        : never;
type ObjectUnknownFields<Actual, Expected, Path extends string, Depth extends unknown[]> = [Expected] extends [never]
  ? never
  : string extends KeysOf<Expected>
    ? never
    : {
        [Key in keyof Actual & string]: Key extends KeysOf<Expected>
          ? UnknownFields<Actual[Key], FieldOf<Expected, Key>, `${Path}${Key}.`, [...Depth, unknown]>
          : `${Path}${Key}`;
      }[keyof Actual & string];
type Expected<Key> = Key extends CommandName ? CommandResult<Key> : Key extends NativeCommand ? NativeResults[Key] : never;
type Checked<H> = {
  [Key in keyof H]: Key extends CommandName | NativeCommand
    ? CheckedHandler<H[Key], Key>
    : { 'is not a generated command': Key };
};
type CheckedHandler<Handler, Key> = Handler extends (...args: never[]) => infer Returned
  ? [UnknownFields<Awaited<Returned>, Expected<Key>>] extends [never]
    ? Handler
    : { 'returns fields the generated DTO does not have': UnknownFields<Awaited<Returned>, Expected<Key>> }
  : Handler;

/** The page-side bridge, available to handlers and as `window.__LATTICE_IPC__`. */
export interface MockIpc {
  /** Handlers by command name; the latest registration wins. */
  handlers: MockHandlers;
  /** Answer an invoke the way the renderer's would be answered. */
  invoke(command: string, args?: MockArgs): Promise<unknown>;
  /** Deliver an event to the renderer's listeners, as the backend would. */
  emit(event: string, payload?: unknown): void;
  /** Send a message on a Channel the renderer passed as an argument. */
  send(channel: unknown, message: unknown): void;
  /** Every command the renderer invoked, by name, in order. */
  calls: Array<{ command: string; args: MockArgs }>;
  /** Every event the renderer emitted, in order. */
  emitted: Array<{ event: string; payload: unknown }>;
  /** Commands no fixture answered. */
  unsupported: string[];
  /** Commands answered in Node, by name. */
  served: string[];
}

declare global {
  interface Window {
    __LATTICE_IPC__: MockIpc;
    __latticeServeCommand?: (command: string, args: MockArgs) => Promise<unknown>;
    __latticeReportUnsupported?: (command: string) => Promise<void>;
  }
}

/**
 * Install the bridge. Call before any `mockCommands` or `serveCommands`.
 * Returns the commands no fixture answered, kept in Node so a reload does
 * not lose them.
 */
export async function installTauriMock(page: Page) {
  const unsupported: string[] = [];
  await page.exposeFunction('__latticeReportUnsupported', (command: string) => {
    unsupported.push(command);
  });
  await page.addInitScript(() => {
    let nextId = 1;
    const callbacks = new Map<number, (payload: unknown) => unknown>();
    const listeners = new Map<number, { event: string; handler: number }>();
    const channelIndex = new Map<number, number>();
    const route = (command: string) => {
      const match = /^plugin:([^|]+)\|(.+)$/.exec(command);
      return match ? { plugin: match[1], name: match[2] } : { plugin: null, name: command };
    };

    const ipc: MockIpc = {
      handlers: {},
      calls: [],
      emitted: [],
      unsupported: [],
      served: [],
      emit(event, payload) {
        for (const [id, listener] of listeners) {
          if (listener.event === event) void callbacks.get(listener.handler)?.({ event, id, payload });
        }
      },
      send(channel, message) {
        const id = (channel as { id: number }).id;
        const index = channelIndex.get(id) ?? 0;
        channelIndex.set(id, index + 1);
        void callbacks.get(id)?.({ index, message });
      },
      async invoke(command, args = {}) {
        const { plugin, name } = route(command);
        if (plugin === 'event') {
          if (name === 'listen') {
            const id = nextId++;
            listeners.set(id, { event: String(args.event), handler: Number(args.handler) });
            return id;
          }
          if (name === 'unlisten') {
            listeners.delete(Number(args.eventId));
            return undefined;
          }
          if (name === 'emit') {
            ipc.emitted.push({ event: String(args.event), payload: args.payload });
            ipc.emit(String(args.event), args.payload);
            return undefined;
          }
        }
        const key = plugin === 'dialog' || plugin === 'app' ? `${plugin}|${name}` : name;
        ipc.calls.push({ command: key, args });
        const handler = (ipc.handlers as Record<string, ((args: MockArgs, ipc: MockIpc) => unknown) | undefined>)[key];
        if (handler) return handler(args, ipc);
        if (ipc.served.includes(key) && window.__latticeServeCommand) return window.__latticeServeCommand(key, args);
        ipc.unsupported.push(key);
        void window.__latticeReportUnsupported?.(key);
        throw new Error(`No e2e fixture answers ${command}`);
      },
    };

    Object.defineProperty(window, '__LATTICE_IPC__', { configurable: true, value: ipc });
    Object.defineProperty(window, 'isTauri', { configurable: true, value: true });
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: {
        metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
        transformCallback(callback: (payload: unknown) => unknown, once = false) {
          const id = nextId++;
          callbacks.set(id, once ? (payload) => { callbacks.delete(id); return callback(payload); } : callback);
          return id;
        },
        unregisterCallback(id: number) {
          callbacks.delete(id);
        },
        invoke: (command: string, args?: MockArgs) => ipc.invoke(command, args),
      },
    });
    Object.defineProperty(window, '__TAURI_EVENT_PLUGIN_INTERNALS__', {
      configurable: true,
      value: { unregisterListener: (_event: string, id: number) => listeners.delete(id) },
    });
  });
  await mockCommands(page, () => ({
    check_first_run_status: () => JSON.stringify({
      needs_setup: false,
      recommended_model_id: null,
      recommended_model_name: null,
      estimated_size_bytes: null,
    }),
  }), undefined);
  return { unsupported };
}

/**
 * Answer commands in the page. `factory` is serialized into the page, so it
 * may use only its arguments: `arg` (JSON) and the bridge.
 */
export async function mockCommands<Arg, H extends MockHandlers>(page: Page, factory: (arg: Arg, ipc: MockIpc) => H & Checked<H>, arg: Arg) {
  const serializedArg = arg === undefined ? 'undefined' : JSON.stringify(arg);
  await page.addInitScript({
    content: `(() => {
      const ipc = window.__LATTICE_IPC__;
      Object.assign(ipc.handlers, (${factory.toString()})(${serializedArg}, ipc));
    })();`,
  });
}

const servedByPage = new WeakMap<Page, { handlers: ServedHandlers; calls: Array<{ command: string; args: MockArgs }>; unsupported: string[] }>();

/**
 * Answer commands in Node, where state outlives a page reload the way the
 * database does. `define` returns the handlers; it is a function, like
 * `mockCommands`' factory, so their results can be checked against the
 * generated DTOs. Returns the shared log of served calls.
 */
export async function serveCommands<H extends ServedHandlers>(page: Page, define: () => H & Checked<H>) {
  const handlers = define();
  let served = servedByPage.get(page);
  if (!served) {
    const state = { handlers: {} as ServedHandlers, calls: [] as Array<{ command: string; args: MockArgs }>, unsupported: [] as string[] };
    servedByPage.set(page, state);
    served = state;
    await page.exposeFunction('__latticeServeCommand', async (command: string, args: MockArgs = {}) => {
      state.calls.push({ command, args });
      const handler = (state.handlers as Record<string, ((args: MockArgs) => unknown) | undefined>)[command];
      if (!handler) {
        state.unsupported.push(command);
        throw new Error(`No e2e fixture serves ${command}`);
      }
      return handler(args);
    });
  }
  Object.assign(served.handlers, handlers);
  await page.addInitScript((names) => {
    window.__LATTICE_IPC__.served.push(...names);
  }, Object.keys(handlers));
  return served;
}
