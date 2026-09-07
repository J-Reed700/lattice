/**
 * Auto-mock for VaultAPI
 * This will be automatically used by Vitest when vi.mock() is called
 */
import { createVaultAPIMock } from '../../tests/mocks/latticeApiMock';

// Create and export a fresh mock instance
const vaultApiMock = createVaultAPIMock();
export const VaultAPI = vaultApiMock;
export default vaultApiMock;
