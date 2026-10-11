import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const run = promisify(execFile);

// RSS/working-set regression evidence, not a native heap leak detector.
// macOS WebKit XPC processes can be outside this parent tree and require
// Instruments; the report deliberately identifies the measured process IDs.
export async function sampleProcessTree(rootPid) {
  let rows;
  if (process.platform === 'win32') {
    const { stdout } = await run('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
      'Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,WorkingSetSize,@{n="Created";e={if ($_.CreationDate) { $_.CreationDate.ToFileTimeUtc() } else { $null }}} | ConvertTo-Json -Compress'], { timeout: 10000 });
    rows = JSON.parse(stdout).map(row => ({ pid: row.ProcessId, parent: row.ParentProcessId, bytes: Number(row.WorkingSetSize), created: row.Created ?? null }));
  } else {
    const { stdout } = await run('ps', ['-axo', 'pid=,ppid=,rss='], { timeout: 10000 });
    rows = stdout.trim().split('\n').map(line => {
      const [pid, parent, kib] = line.trim().split(/\s+/).map(Number);
      return { pid, parent, bytes: kib * 1024 };
    });
  }
  // Windows reuses a dead parent's PID, so an unrelated orphan (e.g. a
  // toolchain's vctip.exe) can name the app as its parent. A real child is
  // never created before its parent.
  const created = new Map(rows.map(row => [row.pid, row.created ?? null]));
  const bornAfterParent = row => row.created == null || created.get(row.parent) == null
    || row.created >= created.get(row.parent);
  const included = new Set([rootPid]);
  for (let changed = true; changed;) {
    changed = false;
    for (const row of rows) {
      if (!included.has(row.pid) && included.has(row.parent) && bornAfterParent(row)) {
        included.add(row.pid);
        changed = true;
      }
    }
  }
  const processes = rows.filter(row => included.has(row.pid));
  if (!processes.some(row => row.pid === rootPid)) throw new Error('Candidate process exited during resource sampling');
  return { processes, residentBytes: processes.reduce((sum, row) => sum + row.bytes, 0) };
}

// Opt-in local diagnostic: Apple leaks examines the native allocator graph.
// This complements RSS trends; it does not inspect WebKit XPC or Metal heaps.
export async function scanNativeLeaks(rootPid) {
  if (process.platform !== 'darwin') return { available: false, reason: 'macOS only' };
  try {
    const { stdout, stderr } = await run('leaks', ['--noContent', String(rootPid)], { timeout: 45000, maxBuffer: 4 * 1024 * 1024 });
    return { available: true, exitCode: 0, output: `${stdout}\n${stderr}` };
  } catch (error) {
    return { available: true, exitCode: error.code ?? null, output: `${error.stdout ?? ''}\n${error.stderr ?? ''}`, error: error.message };
  }
}
