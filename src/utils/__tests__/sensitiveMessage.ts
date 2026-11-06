export const SENSITIVE_MESSAGE =
  'Failed for alice@example.com with token 0123456789abcdef0123456789abcdef at 192.168.1.42: C:\\Users\\alice\\secret.txt';

export const SANITIZED_MESSAGE = 'Failed for [EMAIL] with token [TOKEN] at [IP]: [PATH]';

export const SENSITIVE_PARTS = [
  'alice@example.com',
  '0123456789abcdef0123456789abcdef',
  '192.168.1.42',
  'secret.txt',
];
