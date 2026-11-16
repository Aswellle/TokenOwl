param([Parameter(Mandatory = $true)][int]$ProcessId)

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class TokenOwlWin32 {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")]
    public static extern IntPtr GetWindowLongPtr(IntPtr hWnd, int index);
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

# Without this, Windows virtualizes GetWindowRect for the 150% display and a
# 56-logical-px tool window reports 56 physical px instead of 84.
[void][TokenOwlWin32]::SetProcessDPIAware()

$rows = New-Object System.Collections.ArrayList
$callback = [TokenOwlWin32+EnumWindowsProc]{
    param([IntPtr]$handle, [IntPtr]$lParam)
    $owner = 0
    [void][TokenOwlWin32]::GetWindowThreadProcessId($handle, [ref]$owner)
    if ($owner -eq $ProcessId) {
        $title = New-Object System.Text.StringBuilder 512
        [void][TokenOwlWin32]::GetWindowText($handle, $title, 512)
        $rect = New-Object TokenOwlWin32+RECT
        [void][TokenOwlWin32]::GetWindowRect($handle, [ref]$rect)
        $exStyle = [TokenOwlWin32]::GetWindowLongPtr($handle, -20).ToInt64()
        $style = [TokenOwlWin32]::GetWindowLongPtr($handle, -16).ToInt64()
        [void]$rows.Add([pscustomobject]@{
            handle = $handle.ToInt64()
            title = $title.ToString()
            visible = [TokenOwlWin32]::IsWindowVisible($handle)
            x = $rect.Left
            y = $rect.Top
            width = $rect.Right - $rect.Left
            height = $rect.Bottom - $rect.Top
            style = $style
            exStyle = $exStyle
        })
    }
    return $true
}
[void][TokenOwlWin32]::EnumWindows($callback, [IntPtr]::Zero)
ConvertTo-Json -InputObject @($rows) -Compress
