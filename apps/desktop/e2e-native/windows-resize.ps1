param(
    [Parameter(Mandatory = $true)][int]$ProcessId,
    [Parameter(Mandatory = $true)][string]$Title,
    [Parameter(Mandatory = $true)][int]$Width,
    [Parameter(Mandatory = $true)][int]$Height
)

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class TokenOwlResize {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")]
    public static extern bool SetWindowPos(IntPtr hWnd, IntPtr insertAfter, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")]
    public static extern bool SetProcessDPIAware();
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }
}
"@

[void][TokenOwlResize]::SetProcessDPIAware()

$script:target = [IntPtr]::Zero
$callback = [TokenOwlResize+EnumWindowsProc]{
    param([IntPtr]$handle, [IntPtr]$lParam)
    $owner = 0
    [void][TokenOwlResize]::GetWindowThreadProcessId($handle, [ref]$owner)
    if ($owner -eq $ProcessId) {
        $text = New-Object System.Text.StringBuilder 512
        [void][TokenOwlResize]::GetWindowText($handle, $text, 512)
        if ($text.ToString() -eq $Title) {
            $script:target = $handle
            return $false
        }
    }
    return $true
}
[void][TokenOwlResize]::EnumWindows($callback, [IntPtr]::Zero)

if ($script:target -eq [IntPtr]::Zero) {
    '{"ok":false,"reason":"window not found"}'
    exit 0
}
# SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE
$ok = [TokenOwlResize]::SetWindowPos($script:target, [IntPtr]::Zero, 0, 0, $Width, $Height, 0x16)
Start-Sleep -Milliseconds 150
$rect = New-Object TokenOwlResize+RECT
[void][TokenOwlResize]::GetWindowRect($script:target, [ref]$rect)
[pscustomobject]@{
    ok = $ok
    x = $rect.Left
    y = $rect.Top
    width = $rect.Right - $rect.Left
    height = $rect.Bottom - $rect.Top
} | ConvertTo-Json -Compress
