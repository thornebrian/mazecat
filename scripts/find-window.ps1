# Lists Maze Cat's windows: title, visible or not, position and size. Read-only.
# Run: powershell -ExecutionPolicy Bypass -File scripts\find-window.ps1
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text;
public class MCWin {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out Rect r);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  public struct Rect { public int Left, Top, Right, Bottom; }
}
"@
$procs = Get-Process mazecat -ErrorAction SilentlyContinue
if (-not $procs) { Write-Host 'Maze Cat is not running.'; exit }
$ids = $procs.Id
Write-Host "Maze Cat process id(s): $($ids -join ', ')"
$screen = (Get-CimInstance Win32_VideoController | Select-Object -First 1)
Write-Host "Screen: $($screen.CurrentHorizontalResolution) x $($screen.CurrentVerticalResolution)"
[MCWin]::EnumWindows({
  param($h, $l)
  $p = 0; [void][MCWin]::GetWindowThreadProcessId($h, [ref]$p)
  if ($ids -contains $p) {
    $s = New-Object Text.StringBuilder 200; [void][MCWin]::GetWindowText($h, $s, 200)
    $r = New-Object MCWin+Rect; [void][MCWin]::GetWindowRect($h, [ref]$r)
    if ($s.Length -gt 0) {
      '{0,-28} visible={1,-5} x={2} y={3} w={4} h={5}' -f $s, [MCWin]::IsWindowVisible($h), $r.Left, $r.Top, ($r.Right - $r.Left), ($r.Bottom - $r.Top)
    }
  }
  $true
}, [IntPtr]::Zero) | Out-Null
