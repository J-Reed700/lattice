import { VaultAPI } from '@/lib/api';

/** Asks a llama.cpp server for its models and checks it answers a chat. Changes nothing. */
export const probeLlamaCppServer = (connection: Parameters<typeof VaultAPI.testLlamaCppConnection>[0]) =>
  VaultAPI.testLlamaCppConnection(connection);

/** Sends one test query to a custom search tool. Changes nothing. */
export const probeCustomTool = (request: Parameters<typeof VaultAPI.testCustomTool>[0]) =>
  VaultAPI.testCustomTool(request);
