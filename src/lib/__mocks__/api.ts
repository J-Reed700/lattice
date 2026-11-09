/**
 * Auto-mock for VaultAPI
 * This will be automatically used by Vitest when vi.mock() is called
 */
import { createVaultAPIMock } from '../../tests/mocks/latticeApiMock';

export { isConflict, newId, unwrap } from '../apiResult';

const vaultApiMock = createVaultAPIMock();
export const VaultAPI = vaultApiMock;
export default vaultApiMock;
