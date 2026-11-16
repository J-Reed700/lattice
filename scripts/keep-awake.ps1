# Keeps Windows awake until this process is stopped (ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED).
Add-Type -Namespace Lattice -Name Power -MemberDefinition '[DllImport("kernel32.dll")] public static extern uint SetThreadExecutionState(uint flags);'
[void][Lattice.Power]::SetThreadExecutionState([uint32]2147483651)
while ($true) { Start-Sleep -Seconds 30; [void][Lattice.Power]::SetThreadExecutionState([uint32]2147483651) }
