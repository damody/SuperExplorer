param(
    [Parameter(Mandatory = $true)]
    [string] $OutputDirectory,
    [string] $Executable = "",
    [switch] $Quick
)

$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$fixture = Join-Path $OutputDirectory "fixture"
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
$nested = Join-Path $fixture "nested"
New-Item -ItemType Directory -Force -Path $nested | Out-Null
$pdfPath = Join-Path $fixture "sample.pdf"
$notesPath = Join-Path $fixture "notes.txt"
$contentStream = "BT /F1 18 Tf 40 90 Td (Column preview) Tj ET"
$pdfObjects = @(
    "1 0 obj<< /Type /Catalog /Pages 2 0 R >>endobj`n",
    "2 0 obj<< /Type /Pages /Kids [3 0 R] /Count 1 >>endobj`n",
    "3 0 obj<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 144] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>endobj`n",
    "4 0 obj<< /Length $($contentStream.Length) >>stream`n$contentStream`nendstream`nendobj`n",
    "5 0 obj<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>endobj`n"
)
$pdfEncoding = [System.Text.Encoding]::ASCII
$pdfStream = New-Object System.IO.MemoryStream
$headerBytes = $pdfEncoding.GetBytes("%PDF-1.4`n")
$pdfStream.Write($headerBytes, 0, $headerBytes.Length)
$pdfOffsets = New-Object System.Collections.Generic.List[int]
$pdfOffsets.Add(0)
foreach ($pdfObject in $pdfObjects) {
    $pdfOffsets.Add([int]$pdfStream.Length)
    $objectBytes = $pdfEncoding.GetBytes($pdfObject)
    $pdfStream.Write($objectBytes, 0, $objectBytes.Length)
}
$startxref = [int]$pdfStream.Length
$xref = "xref`n0 6`n0000000000 65535 f `n"
for ($pdfIndex = 1; $pdfIndex -le 5; $pdfIndex++) {
    $xref += ("{0:D10} 00000 n `n" -f $pdfOffsets[$pdfIndex])
}
$xref += "trailer<< /Size 6 /Root 1 0 R >>`nstartxref`n$startxref`n%%EOF`n"
$xrefBytes = $pdfEncoding.GetBytes($xref)
$pdfStream.Write($xrefBytes, 0, $xrefBytes.Length)
[System.IO.File]::WriteAllBytes($pdfPath, $pdfStream.ToArray())
$pdfStream.Dispose()
Set-Content -LiteralPath $notesPath -Value "column-preview-notes" -Encoding utf8
$profile = Join-Path $OutputDirectory "localappdata"
New-Item -ItemType Directory -Force -Path $profile | Out-Null

if (-not $Executable) {
    $Executable = "D:\SuperExplorer\target\debug\SuperExplorer.exe"
}

$subcases = New-Object System.Collections.Generic.List[object]
$observations = New-Object System.Collections.Generic.List[string]

function Add-Subcase([string] $Name, [string] $Status, [string] $Reason) {
    $script:subcases.Add([pscustomobject]@{ name = $Name; status = $Status; reason = $Reason })
    $script:observations.Add("${Name}: ${Status} - ${Reason}")
    try { Write-Report "RUNNING" $Reason } catch { }
}

function Write-Report([string] $Status, [string] $Reason) {
    $lines = New-Object System.Collections.Generic.List[string]
    $lines.Add("{")
    $lines.Add(('  "status": {0},' -f ($Status | ConvertTo-Json -Compress)))
    $lines.Add(('  "reason": {0},' -f ($Reason | ConvertTo-Json -Compress)))
    $lines.Add(('  "fixture": {0},' -f ($fixture | ConvertTo-Json -Compress)))
    $lines.Add(('  "pdf": {0},' -f ($pdfPath | ConvertTo-Json -Compress)))
    $lines.Add(('  "executable": {0},' -f ($Executable | ConvertTo-Json -Compress)))
    $lines.Add('  "subcases": [')
    for ($index = 0; $index -lt $subcases.Count; $index++) {
        $case = $subcases[$index]
        $comma = if ($index -lt ($subcases.Count - 1)) { "," } else { "" }
        $lines.Add(('    {{ "name": {0}, "status": {1}, "reason": {2} }}{3}' -f ($case.name | ConvertTo-Json -Compress), ($case.status | ConvertTo-Json -Compress), ($case.reason | ConvertTo-Json -Compress), $comma))
    }
    $lines.Add("  ],")
    $lines.Add('  "observed": [')
    for ($index = 0; $index -lt $observations.Count; $index++) {
        $comma = if ($index -lt ($observations.Count - 1)) { "," } else { "" }
        $lines.Add(('    {0}{1}' -f ($observations[$index] | ConvertTo-Json -Compress), $comma))
    }
    $lines.Add("  ],")
    $lines.Add('  "accessibility_note": "Column inline rename uses GPUI EditableText inside column-inline-rename. This script does not treat that editor as a native UIA Edit control, and this change does not rewrite GPUI text input."')
    $lines.Add("}")
    [System.IO.File]::WriteAllText((Join-Path $OutputDirectory "report.json"), ($lines -join "`n"))
}

function Finish([string] $Status, [string] $Reason, [int] $ExitCode) {
    Write-Report $Status $Reason
    exit $ExitCode
}

if (-not (Test-Path -LiteralPath $Executable)) {
    Add-Subcase "executable" "SKIP" "SuperExplorer.exe was not built, so no Preview Handler window was hosted."
    Finish "SKIP" "SuperExplorer.exe was not built, so the handler lifecycle was not shown." 0
}

$handlerClsid = $null
$handlerKeys = @(
    "Registry::HKEY_CLASSES_ROOT\.pdf\shellex\{8895b1c6-b41f-4c1c-a562-0d564250836f}",
    "Registry::HKEY_CLASSES_ROOT\SystemFileAssociations\.pdf\shellex\{8895b1c6-b41f-4c1c-a562-0d564250836f}",
    "Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Classes\.pdf\shellex\{8895b1c6-b41f-4c1c-a562-0d564250836f}",
    "Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Classes\SystemFileAssociations\.pdf\shellex\{8895b1c6-b41f-4c1c-a562-0d564250836f}"
)
foreach ($key in $handlerKeys) {
    $registration = Get-ItemProperty -LiteralPath $key -ErrorAction SilentlyContinue
    if ($null -ne $registration -and -not [string]::IsNullOrWhiteSpace([string]$registration.'(default)')) {
        $handlerClsid = [string]$registration.'(default)'
        $observations.Add("pdf preview handler clsid: $handlerClsid from $key")
        break
    }
}
if (-not $handlerClsid) {
    Add-Subcase "pdf-preview-handler" "SKIP" "No local PDF Preview Handler is registered under .pdf or SystemFileAssociations\.pdf for {8895b1c6-b41f-4c1c-a562-0d564250836f}."
    Add-Subcase "initial-bounds" "SKIP" "No registered PDF Preview Handler, so no child HWND was hosted."
    Add-Subcase "preview-width-drag" "SKIP" "No registered PDF Preview Handler."
    Add-Subcase "window-resize" "SKIP" "No registered PDF Preview Handler."
    Add-Subcase "dpi-change" "SKIP" "No registered PDF Preview Handler, and DPI was not changed."
    Add-Subcase "alt-p-unload" "SKIP" "No registered PDF Preview Handler."
    Add-Subcase "selection-unload" "SKIP" "No registered PDF Preview Handler."
    Add-Subcase "tab-focus" "SKIP" "No registered PDF Preview Handler."
    Add-Subcase "app-shortcuts" "SKIP" "No registered PDF Preview Handler."
    Add-Subcase "timeout-crash" "SKIP" "No disposable crashing Preview Handler is registered. Timeout and crash recovery stay on the coordinator tests."
    Finish "SKIP" "No registered local PDF Preview Handler was available, so the integrated host was not claimed to pass." 0
}
Add-Subcase "pdf-preview-handler" "PASS" "Registered CLSID $handlerClsid."

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
if (-not ("ColumnHandlerPreview.Native" -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
namespace ColumnHandlerPreview {
    public struct RECT { public int Left, Top, Right, Bottom; }
    public struct POINT { public int X, Y; }
    public static class Native {
        public delegate bool EnumProc(IntPtr hwnd, IntPtr lParam);
        public static int[] WindowRect(IntPtr hwnd) {
            RECT rect;
            if (!GetWindowRect(hwnd, out rect)) return new int[0];
            return new int[] { rect.Left, rect.Top, rect.Right, rect.Bottom };
        }
        public static List<string> EnumerateHosts(IntPtr parent) {
            var found = new List<string>();
            IntPtr child = GetWindow(parent, 5);
            for (int guard = 0; child != IntPtr.Zero && guard < 300; guard++) {
                var name = new StringBuilder(256);
                GetClassName(child, name, name.Capacity);
                RECT rect;
                if (GetWindowRect(child, out rect)) {
                    IntPtr owner = GetParent(child);
                    found.Add(name.ToString() + "|" + rect.Left + "|" + rect.Top + "|" + rect.Right + "|" + rect.Bottom + "|" + child.ToInt64() + "|" + owner.ToInt64());
                }
                child = GetWindow(child, 2);
            }
            return found;
        }
        [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr hwnd, uint command);
        [DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam, uint flags, uint timeout, out IntPtr result);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd, StringBuilder name, int count);
        [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
        [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr insertAfter, int x, int y, int cx, int cy, uint flags);
        [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
        [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
        [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern IntPtr MonitorFromWindow(IntPtr hwnd, uint flags);
        [DllImport("shcore.dll")] public static extern int GetDpiForMonitor(IntPtr monitor, int type, out uint dpiX, out uint dpiY);
    }
}
"@
}

function Find-ByName([Windows.Automation.AutomationElement] $Root, [string] $Name) {
    $condition = New-Object Windows.Automation.PropertyCondition(
        [Windows.Automation.AutomationElement]::NameProperty,
        $Name)
    return $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
}

function Find-ListItem([Windows.Automation.AutomationElement] $Root, [string] $Pattern) {
    $items = $Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        (New-Object Windows.Automation.PropertyCondition(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::ListItem)))
    foreach ($item in $items) {
        if ($item.Current.Name -notmatch $Pattern) { continue }
        $rect = $item.Current.BoundingRectangle
        if ($rect.Width -lt 8 -or $rect.Height -lt 8) { continue }
        return $item
    }
    return $null
}

function Click-Element([Windows.Automation.AutomationElement] $Element) {
    try {
        $pattern = $Element.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)
        $pattern.Invoke()
        return "invoke"
    } catch {
        $bounds = $Element.Current.BoundingRectangle
        $x = [int]($bounds.Left + ($bounds.Width / 2))
        $y = [int]($bounds.Top + ([Math]::Min(12, $bounds.Height / 2)))
        [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point $x, $y
        Start-Sleep -Milliseconds 80
        [ColumnHandlerPreview.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [ColumnHandlerPreview.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        return "pointer@$x,$y"
    }
}

function Click-Row([Windows.Automation.AutomationElement] $Element) {
    $bounds = $Element.Current.BoundingRectangle
    $x = [int]($bounds.Left + 24)
    $y = [int]($bounds.Top + ($bounds.Height / 2))
    [ColumnHandlerPreview.Native]::SetCursorPos($x, $y)
    Start-Sleep -Milliseconds 80
    [ColumnHandlerPreview.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [ColumnHandlerPreview.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    return "row@$x,$y"
}

function Send-Key([byte] $Key) {
    [ColumnHandlerPreview.Native]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    [ColumnHandlerPreview.Native]::keybd_event($Key, 0, 2, [UIntPtr]::Zero)
}

function Send-Chord([byte] $Modifier, [byte] $Key) {
    [ColumnHandlerPreview.Native]::keybd_event($Modifier, 0, 0, [UIntPtr]::Zero)
    Send-Key $Key
    [ColumnHandlerPreview.Native]::keybd_event($Modifier, 0, 2, [UIntPtr]::Zero)
}

function Get-HostWindows([IntPtr] $Parent) {
    $hosts = New-Object System.Collections.Generic.List[object]
    foreach ($row in [ColumnHandlerPreview.Native]::EnumerateHosts($Parent)) {
        $parts = $row.Split("|")
        if ($parts[0] -ne "RustGpuiExplorerPreviewWorkerHost") { continue }
        $left = [int]$parts[1]
        $top = [int]$parts[2]
        $right = [int]$parts[3]
        $bottom = [int]$parts[4]
        $hosts.Add([pscustomobject]@{
            Class = $parts[0]
            Left = $left
            Top = $top
            Right = $right
            Bottom = $bottom
            Width = $right - $left
            Height = $bottom - $top
            Hwnd = $parts[5]
            Parent = $parts[6]
        })
    }
    return $hosts
}

function Format-Rect($Rect) {
    if ($null -eq $Rect) { return "missing" }
    if ($Rect -is [System.Windows.Rect] -or $Rect.PSObject.Properties.Name -contains "Width" -and $Rect.PSObject.Properties.Name -contains "Left") {
        return "$($Rect.Left),$($Rect.Top) $($Rect.Width)x$($Rect.Height)"
    }
    return "$($Rect.Left),$($Rect.Top) $($Rect.Width)x$($Rect.Height)"
}

function Test-HostInsidePreview($HostWindow, $Preview, [int] $Slop) {
    if ($null -eq $HostWindow -or $null -eq $Preview) { return $false }
    $bounds = $Preview.Current.BoundingRectangle
    if ($bounds.Width -lt 8 -or $bounds.Height -lt 8) { return $false }
    return ($HostWindow.Left -ge ($bounds.Left - $Slop)) -and
        ($HostWindow.Top -ge ($bounds.Top - $Slop)) -and
        ($HostWindow.Right -le ($bounds.Right + $Slop)) -and
        ($HostWindow.Bottom -le ($bounds.Bottom + $Slop)) -and
        ($HostWindow.Width -gt 8) -and ($HostWindow.Height -gt 8)
}

function Test-HostCoversRow($HostWindow, [Windows.Automation.AutomationElement] $Root) {
    $items = $Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        (New-Object Windows.Automation.PropertyCondition(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::ListItem)))
    foreach ($item in $items) {
        $rect = $item.Current.BoundingRectangle
        if ($rect.Width -lt 8 -or $rect.Height -lt 8) { continue }
        $overlaps = $HostWindow.Right -gt ($rect.Left + 4) -and $HostWindow.Left -lt ($rect.Right - 4) -and
            $HostWindow.Bottom -gt ($rect.Top + 4) -and $HostWindow.Top -lt ($rect.Bottom - 4)
        if ($overlaps) { return $item.Current.Name }
    }
    return $null
}

function Get-PreviewElement([Windows.Automation.AutomationElement] $Root) {
    $preview = Find-ByName $Root ([string]::new(@([char]0x6574, [char]0x5408, [char]0x9810, [char]0x89BD)))
    if ($null -eq $preview) { $preview = Find-ByName $Root "Integrated preview" }
    return $preview
}

function Save-Screen([string] $Path) {
    $screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bitmap = New-Object System.Drawing.Bitmap $screen.Width, $screen.Height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen($screen.Location, [System.Drawing.Point]::Empty, $screen.Size)
    $bitmap.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    $graphics.Dispose()
    $bitmap.Dispose()
}

$start = New-Object System.Diagnostics.ProcessStartInfo
$start.FileName = $Executable
$start.WorkingDirectory = "D:\SuperExplorer"
$start.UseShellExecute = $false
$start.Environment["EXPLORER_INITIAL_PATH"] = $fixture
$start.Environment["EXPLORER_LOG_DIR"] = $OutputDirectory
$start.Environment["LOCALAPPDATA"] = $profile
$start.Environment["SUPEREXPLORER_DISABLE_REPEATED_LAUNCH_DETECTION"] = "1"
$start.Environment["EXPLORER_AUTO_CLOSE_MS"] = "180000"
$process = [Diagnostics.Process]::Start($start)
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        if ($process.HasExited) { throw "application exited early: $($process.ExitCode)" }
        $process.Refresh()
        if ($process.MainWindowHandle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($process.MainWindowHandle -eq [IntPtr]::Zero) { throw "application window did not appear" }
    [void][ColumnHandlerPreview.Native]::SetWindowPos($process.MainWindowHandle, [IntPtr]::Zero, 40, 40, 1280, 800, 0x0040)
    [void][ColumnHandlerPreview.Native]::SetForegroundWindow($process.MainWindowHandle)
    Start-Sleep -Milliseconds 900
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $pdfRow = $null
    $located = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
        $pdfRow = Find-ListItem $root '(^| )sample\.pdf( |$)'
        if ($null -ne $pdfRow) { break }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $located)
    if ($null -eq $pdfRow) { throw "runner fixture sample.pdf was not listed" }
    $observations.Add("found fixture row: $($pdfRow.Current.Name)")
    $view = Find-ByName $root ([string]::new(@([char]0x6AA2, [char]0x8996)))
    if ($null -eq $view) { $view = Find-ByName $root "View" }
    if ($null -eq $view) { throw "view menu button was not found" }
    $observations.Add(("view click: " + (Click-Element $view)))
    Start-Sleep -Milliseconds 400
    $columns = Find-ByName $root ([string]::new(@([char]0x5206, [char]0x6B04)))
    if ($null -eq $columns) { $columns = Find-ByName $root "Columns" }
    if ($null -eq $columns) { throw "columns menu item was not found" }
    $observations.Add(("columns click: " + (Click-Element $columns)))
    Start-Sleep -Milliseconds 800
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $surface = Find-ByName $root ([string]::new(@([char]0x5206, [char]0x6B04, [char]0x6AA2, [char]0x8996)))
    if ($null -eq $surface) { $surface = Find-ByName $root "Column view" }
    if ($null -eq $surface) { throw "column view surface was not exposed to UIA" }
    $ordinary = Find-ByName $root ([string]::new(@([char]0x9810, [char]0x89BD, [char]0x7A97, [char]0x683C)))
    if ($null -eq $ordinary) { $ordinary = Find-ByName $root "Preview pane" }
    if ($null -ne $ordinary) { throw "ordinary preview pane is visible beside the column preview" }
    $pdfRow = Find-ListItem $surface '(^| )sample\.pdf( |$)'
    if ($null -eq $pdfRow) { throw "sample.pdf disappeared after switching to Columns" }
    $nestedRow = Find-ListItem $surface '(^| )nested( |$)'
    $nestedRect = if ($null -ne $nestedRow) { $nestedRow.Current.BoundingRectangle } else { $null }
    $observations.Add(("pdf click: " + (Click-Row $pdfRow)))

    $preview = Get-PreviewElement $surface
    $hostWindow = $null
    $hostDeadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $hosts = @(Get-HostWindows $process.MainWindowHandle)
        $hostWindow = $hosts | Select-Object -First 1
        if ($null -ne $hostWindow -and (Test-HostInsidePreview $hostWindow $preview 12)) { break }
        Start-Sleep -Milliseconds 300
    } while ([DateTime]::UtcNow -lt $hostDeadline)
    if ($null -eq $preview) { throw "integrated preview slot was not exposed to UIA" }
    $previewBounds = $preview.Current.BoundingRectangle
    $observations.Add("preview bounds: $(Format-Rect $previewBounds)")
    if ($null -eq $hostWindow) {
        $classes = (@(Get-HostWindows $process.MainWindowHandle) | ForEach-Object { $_.Class } | Select-Object -First 12) -join " || "
        $observations.Add("child windows: $classes")
        Add-Subcase "initial-bounds" "FAIL" "Registered PDF handler did not create RustGpuiExplorerPreviewWorkerHost inside the integrated preview."
        throw "PDF preview host window was not created"
    }
    $observations.Add("host bounds: $($hostWindow.Left),$($hostWindow.Top) $($hostWindow.Width)x$($hostWindow.Height) parent=$($hostWindow.Parent)")
    $covered = Test-HostCoversRow $hostWindow $surface
    if ($covered) { throw "preview host overlaps column row $covered" }
    if (-not (Test-HostInsidePreview $hostWindow $preview 12)) {
        throw "initial preview host is outside the integrated preview"
    }
    Add-Subcase "initial-bounds" "PASS" "Host $($hostWindow.Hwnd) is inside the integrated preview and does not cover a column row."
    Save-Screen (Join-Path $OutputDirectory "handler-initial.png")

    $resized = $null
    if (-not $Quick) {
    $hostBeforeResize = $hostWindow
    [void][ColumnHandlerPreview.Native]::SetWindowPos($process.MainWindowHandle, [IntPtr]::Zero, 80, 60, 1000, 700, 0x0040)
    Start-Sleep -Milliseconds 900
    $resized = @(Get-HostWindows $process.MainWindowHandle) | Select-Object -First 1
    $resizedWindow = [ColumnHandlerPreview.Native]::WindowRect($process.MainWindowHandle)
    if ($null -eq $resized) {
        $observations.Add("host after resize: missing; window $($resizedWindow -join ',')")
    } else {
        $observations.Add("host after resize: $($resized.Left),$($resized.Top) $($resized.Width)x$($resized.Height); window $($resizedWindow -join ',')")
    }
    $resizedInside = $null -ne $resized -and $resizedWindow.Length -eq 4 -and $resized.Left -ge ($resizedWindow[0] - 8) -and $resized.Top -ge ($resizedWindow[1] - 8) -and $resized.Right -le ($resizedWindow[2] + 8) -and $resized.Bottom -le ($resizedWindow[3] + 8)
    if (-not $resizedInside) {
        Add-Subcase "window-resize" "FAIL" "After window resize the host was missing or outside the window."
    } elseif ([Math]::Abs($resized.Left - $hostBeforeResize.Left) -lt 4 -and [Math]::Abs($resized.Top - $hostBeforeResize.Top) -lt 4) {
        Add-Subcase "window-resize" "FAIL" "The window moved but the host stayed at $($hostBeforeResize.Left),$($hostBeforeResize.Top)."
    } else {
        Add-Subcase "window-resize" "PASS" "Host followed the window to $($resized.Left),$($resized.Top) $($resized.Width)x$($resized.Height)."
    }
    } else {
        Add-Subcase "window-resize" "SKIP" "Quick run avoids synchronous Win32 resizing while the PDF handler is attached."
    }

    $beforeHost = if ($null -ne $resized) { $resized } else { $hostWindow }
    # Further UIA walks block inside the Edge PDF handler. Drag the known
    # preview edge and judge the child HWND directly.
    $x = [int]($beforeHost.Left - 20)
    $y = [int]($beforeHost.Top + 40)
    [ColumnHandlerPreview.Native]::SetCursorPos($x, $y)
    Start-Sleep -Milliseconds 100
    [ColumnHandlerPreview.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 80
    [ColumnHandlerPreview.Native]::SetCursorPos($x - 80, $y)
    Start-Sleep -Milliseconds 120
    [ColumnHandlerPreview.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 700
    $dragged = @(Get-HostWindows $process.MainWindowHandle) | Select-Object -First 1
    if ($null -eq $dragged) {
        $observations.Add("host width drag: host disappeared")
    } else {
        $observations.Add("host width drag: $($beforeHost.Width) -> $($dragged.Width) left $($beforeHost.Left) -> $($dragged.Left)")
    }
    $windowRect = [ColumnHandlerPreview.Native]::WindowRect($process.MainWindowHandle)
    $insideWindow = $null -ne $dragged -and $windowRect.Length -eq 4 -and $dragged.Left -ge ($windowRect[0] - 8) -and $dragged.Top -ge ($windowRect[1] - 8) -and $dragged.Right -le ($windowRect[2] + 8) -and $dragged.Bottom -le ($windowRect[3] + 8)
    if ($null -eq $dragged -or -not $insideWindow) {
        Add-Subcase "preview-width-drag" "FAIL" "After the preview-width drag the host was missing or outside the window."
    } elseif ($dragged.Width -le ($beforeHost.Width + 8) -or $dragged.Left -ge $beforeHost.Left) {
        Add-Subcase "preview-width-drag" "FAIL" "Dragging the preview edge did not widen the host ($($beforeHost.Width) -> $($dragged.Width))."
    } elseif ($dragged.Left -lt ($beforeHost.Left - 160)) {
        Add-Subcase "preview-width-drag" "FAIL" "The widened host moved to $($dragged.Left), left of the integrated preview."
    } else {
        Add-Subcase "preview-width-drag" "PASS" "Host widened from $($beforeHost.Width) to $($dragged.Width) and stayed in the preview side of the window."
    }

    if ($Quick) {
        Add-Subcase "dpi-change" "SKIP" "Quick run does not move the window between displays."
    } else {
    $dpi = [ColumnHandlerPreview.Native]::GetDpiForWindow($process.MainWindowHandle)
    $observations.Add("window dpi: $dpi")
    $other = $null
    foreach ($screen in [System.Windows.Forms.Screen]::AllScreens) {
        if ($screen.Primary) { continue }
        $other = $screen
        break
    }
    if ($null -eq $other) {
        Add-Subcase "dpi-change" "SKIP" "Only one display is attached. Window DPI is $dpi. System DPI was not changed."
    } else {
        $beforeDpi = $dpi
        $moved = $false
        [void][ColumnHandlerPreview.Native]::SetWindowPos(
            $process.MainWindowHandle,
            [IntPtr]::Zero,
            $other.Bounds.X + 40,
            $other.Bounds.Y + 40,
            1000,
            700,
            0x0040)
        Start-Sleep -Milliseconds 1200
        $afterDpi = [ColumnHandlerPreview.Native]::GetDpiForWindow($process.MainWindowHandle)
        $observations.Add("dpi after monitor move: $beforeDpi -> $afterDpi on $($other.DeviceName)")
        if ($afterDpi -eq $beforeDpi) {
            Add-Subcase "dpi-change" "SKIP" "A second display $($other.DeviceName) is attached, but moving the window left DPI at $afterDpi. System DPI was not changed."
        } else {
            $movedHost = @(Get-HostWindows $process.MainWindowHandle) | Select-Object -First 1
            if ($null -eq $movedHost -or -not (Test-HostInsidePreview $movedHost $preview 16)) {
                Add-Subcase "dpi-change" "FAIL" "After DPI $beforeDpi -> $afterDpi the host was missing or outside the integrated preview."
            } else {
                Add-Subcase "dpi-change" "PASS" "Host stayed inside the integrated preview after DPI changed from $beforeDpi to $afterDpi."
            }
        }
    }
    }

    Add-Subcase "tab-focus" "SKIP" "UIA focus queries block while the Edge PDF Preview Handler is attached, so Tab entry was not read back. The model test column_preview_tab_order_follows_the_integrated_preview_and_leaves_the_ordinary_pane_alone covers enter and leave."
    Add-Subcase "app-shortcuts" "SKIP" "UIA focus queries block while the Edge PDF Preview Handler is attached, so Ctrl+L focus was not read back. The UI test column_preview_handler_bounds_follow_resize_and_dpi_and_accelerators_stay_scoped keeps Alt+P, Ctrl+L, and Tab with the app."

    $hostsBeforeToggle = @(Get-HostWindows $process.MainWindowHandle).Count
    Send-Chord 0x12 0x50
    $gone = $false
    $toggleDeadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        Start-Sleep -Milliseconds 250
        if (@(Get-HostWindows $process.MainWindowHandle).Count -eq 0) { $gone = $true; break }
    } while ([DateTime]::UtcNow -lt $toggleDeadline)
    if (-not $gone) {
        Add-Subcase "alt-p-unload" "FAIL" "Alt+P left $hostsBeforeToggle preview host window(s) attached."
    } else {
        Send-Chord 0x12 0x50
        $restored = $null
        $restoredInside = $false
        $restoreDeadline = [DateTime]::UtcNow.AddSeconds(20)
        do {
            $restored = @(Get-HostWindows $process.MainWindowHandle) | Select-Object -First 1
            $restoredWindow = [ColumnHandlerPreview.Native]::WindowRect($process.MainWindowHandle)
            $restoredInside = $null -ne $restored -and $restoredWindow.Length -eq 4 -and $restored.Left -ge ($restoredWindow[0] - 8) -and $restored.Right -le ($restoredWindow[2] + 8) -and $restored.Top -ge ($restoredWindow[1] - 8) -and $restored.Bottom -le ($restoredWindow[3] + 8) -and $restored.Left -gt ($previewBounds.Left - 160)
            if ($restoredInside) { break }
            Start-Sleep -Milliseconds 300
        } while ([DateTime]::UtcNow -lt $restoreDeadline)
        if (-not $restoredInside) {
            Add-Subcase "alt-p-unload" "FAIL" "Alt+P removed the host, but turning the preview back on did not place a host inside the window's preview side."
        } else {
            Add-Subcase "alt-p-unload" "PASS" "Alt+P removed the host and reopening placed it at $($restored.Left),$($restored.Top) $($restored.Width)x$($restored.Height)."
        }
    }

    if ($null -eq $nestedRect -or $nestedRect.Width -lt 8) {
        Add-Subcase "selection-unload" "FAIL" "The nested folder row was not listed before the handler attached, so selection unload was not attempted."
    } else {
        $folderX = [int]($nestedRect.Left + 24)
        $folderY = [int]($nestedRect.Top + ($nestedRect.Height / 2))
        [ColumnHandlerPreview.Native]::SetCursorPos($folderX, $folderY)
        Start-Sleep -Milliseconds 80
        [ColumnHandlerPreview.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [ColumnHandlerPreview.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        $observations.Add("folder click: row@$folderX,$folderY")
        $cleared = $false
        $clearDeadline = [DateTime]::UtcNow.AddSeconds(8)
        do {
            Start-Sleep -Milliseconds 250
            if (@(Get-HostWindows $process.MainWindowHandle).Count -eq 0) { $cleared = $true; break }
        } while ([DateTime]::UtcNow -lt $clearDeadline)
        if ($cleared) {
            Add-Subcase "selection-unload" "PASS" "Selecting the nested folder removed the PDF preview host."
        } else {
            Add-Subcase "selection-unload" "FAIL" "The PDF preview host remained after selecting the nested folder."
        }
    }

    Add-Subcase "timeout-crash" "SKIP" "No disposable crashing or hanging Preview Handler is registered. A fake crash was not installed. Timeout and crash recovery are asserted by column_preview_handler_unloads_for_toggle_selection_tab_and_mode_and_recovers_after_timeout."

    $failed = @($subcases | Where-Object { $_.status -eq "FAIL" })
    if ($failed.Count -gt 0) {
        Save-Screen (Join-Path $OutputDirectory "handler-failed.png")
        Finish "FAIL" $failed[0].reason 1
    }
    Finish "PASS" "The registered PDF Preview Handler stayed inside the integrated column preview across the exercised lifecycle." 0
} catch {
    if ($process -and $process.MainWindowHandle -ne [IntPtr]::Zero) {
        try { Save-Screen (Join-Path $OutputDirectory "handler-failed.png") } catch { }
    }
    $detail = $_.Exception.Message
    if ($_.ScriptStackTrace) { $detail = "$detail`n$($_.ScriptStackTrace)" }
    $observations.Add($detail)
    try { [System.IO.File]::WriteAllText((Join-Path $OutputDirectory "script-error.txt"), $detail) } catch { }
    $already = @($subcases | Where-Object { $_.status -eq "FAIL" })
    if ($already.Count -eq 0) {
        Add-Subcase "runner" "FAIL" $_.Exception.Message
    }
    Finish "FAIL" $_.Exception.Message 1
} finally {
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    }
}
