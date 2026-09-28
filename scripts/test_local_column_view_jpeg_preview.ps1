param(
    [Parameter(Mandatory = $true)]
    [string] $OutputDirectory,
    [string] $Executable = ""
)

$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$fixture = Join-Path $OutputDirectory "fixture"
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
$photoPath = Join-Path $fixture "photo.jpg"
$notesPath = Join-Path $fixture "notes.txt"
Add-Type -AssemblyName System.Drawing
$bitmap = New-Object System.Drawing.Bitmap 64, 32
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.Clear([System.Drawing.Color]::FromArgb(255, 220, 20, 60))
$bitmap.Save($photoPath, [System.Drawing.Imaging.ImageFormat]::Jpeg)
$graphics.Dispose()
$bitmap.Dispose()
Set-Content -Path $notesPath -Value "column-preview-notes" -Encoding utf8
$profile = Join-Path $OutputDirectory "localappdata"
New-Item -ItemType Directory -Force -Path $profile | Out-Null

if (-not $Executable) {
    $Executable = "D:\SuperExplorer\target\debug\SuperExplorer.exe"
}

function Write-Report([string] $Status, [string] $Reason, [System.Collections.Generic.List[string]] $Observed) {
    $report = [ordered]@{
        status = $Status
        reason = $Reason
        fixture = $fixture
        photo = $photoPath
        executable = $Executable
        observed = @($Observed)
    }
    $report | ConvertTo-Json -Depth 6 | Set-Content -Path (Join-Path $OutputDirectory "report.json") -Encoding utf8
}

if (-not (Test-Path -LiteralPath $Executable)) {
    $empty = New-Object System.Collections.Generic.List[string]
    Write-Report "SKIP" "SuperExplorer.exe was not built, so the JPEG preview was not shown." $empty
    exit 0
}

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
if (-not ("ColumnJpegPreview.Native" -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
namespace ColumnJpegPreview {
    public static class Native {
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr insertAfter, int x, int y, int cx, int cy, uint flags);
        [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdc, uint flags);
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

function Find-ListItem([Windows.Automation.AutomationElement] $Root, [string] $Pattern, [Windows.Automation.AutomationElement] $Inside) {
    $limit = $null
    if ($null -ne $Inside) { $limit = $Inside.Current.BoundingRectangle }
    $items = $Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        (New-Object Windows.Automation.PropertyCondition(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::ListItem)))
    foreach ($item in $items) {
        if ($item.Current.Name -notmatch $Pattern) { continue }
        $rect = $item.Current.BoundingRectangle
        if ($rect.Width -lt 8 -or $rect.Height -lt 8) { continue }
        if ($null -ne $limit) {
            $intersects = $rect.Right -gt $limit.Left -and $rect.Left -lt $limit.Right -and $rect.Bottom -gt $limit.Top -and $rect.Top -lt $limit.Bottom
            if (-not $intersects) { continue }
        }
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
        Add-Type -AssemblyName System.Windows.Forms
        [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point $x, $y
        $signature = '[DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);'
        if (-not ("ColumnJpegPreview.Mouse" -as [type])) {
            Add-Type -MemberDefinition $signature -Name "Mouse" -Namespace "ColumnJpegPreview"
        }
        Start-Sleep -Milliseconds 80
        [ColumnJpegPreview.Mouse]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [ColumnJpegPreview.Mouse]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        return "pointer@$x,$y"
    }
}

function Click-Row([Windows.Automation.AutomationElement] $Element) {
    $bounds = $Element.Current.BoundingRectangle
    $x = [int]($bounds.Left + 6)
    $y = [int]($bounds.Top + ($bounds.Height / 2))
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point $x, $y
    if (-not ("ColumnJpegPreview.Mouse" -as [type])) {
        Add-Type -MemberDefinition '[DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);' -Name "Mouse" -Namespace "ColumnJpegPreview"
    }
    Start-Sleep -Milliseconds 80
    [ColumnJpegPreview.Mouse]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [ColumnJpegPreview.Mouse]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    return "row@$x,$y"
}

function Save-Window([IntPtr] $Hwnd, [string] $Path) {
    $bounds = [System.Drawing.Rectangle]::Empty
    try {
        Add-Type -AssemblyName System.Windows.Forms
        $screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
        $bitmap = New-Object System.Drawing.Bitmap $screen.Width, $screen.Height
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        $graphics.CopyFromScreen($screen.Location, [System.Drawing.Point]::Empty, $screen.Size)
        $bitmap.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
        $graphics.Dispose()
        $bitmap.Dispose()
    } catch {
        Set-Content -Path ($Path + ".error.txt") -Value $_.Exception.Message -Encoding utf8
    }
}

$start = New-Object System.Diagnostics.ProcessStartInfo
$start.FileName = $Executable
$start.WorkingDirectory = "D:\SuperExplorer"
$start.UseShellExecute = $false
$start.Environment["EXPLORER_INITIAL_PATH"] = $fixture
$start.Environment["EXPLORER_LOG_DIR"] = $OutputDirectory
$start.Environment["LOCALAPPDATA"] = $profile
# A second ordinary SuperExplorer launch is forced to C:\. Keep this run isolated
# so the runner-owned fixture is the real initial folder.
$start.Environment["SUPEREXPLORER_DISABLE_REPEATED_LAUNCH_DETECTION"] = "1"
$start.Environment["EXPLORER_AUTO_CLOSE_MS"] = "90000"
$process = [Diagnostics.Process]::Start($start)
$observations = New-Object System.Collections.Generic.List[string]
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        if ($process.HasExited) { throw "application exited early: $($process.ExitCode)" }
        $process.Refresh()
        if ($process.MainWindowHandle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($process.MainWindowHandle -eq [IntPtr]::Zero) { throw "application window did not appear" }
    [void][ColumnJpegPreview.Native]::SetWindowPos($process.MainWindowHandle, [IntPtr](-1), 40, 40, 1280, 800, 0x0040)
    [void][ColumnJpegPreview.Native]::SetForegroundWindow($process.MainWindowHandle)
    Start-Sleep -Milliseconds 900
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $photo = $null
    $located = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
        $photo = Find-ListItem $root '(^| )photo\.jpg( |$)'
        if ($null -ne $photo) { break }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $located)
    if ($null -eq $photo) {
        throw "runner fixture photo.jpg was not listed; refusing to treat another folder as the preview target"
    }
    $observations.Add("found fixture row: $($photo.Current.Name)")
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
    $observations.Add("column surface: $($surface.Current.Name)")
    $ordinary = Find-ByName $root ([string]::new(@([char]0x9810, [char]0x89BD, [char]0x7A97, [char]0x683C)))
    if ($null -eq $ordinary) { $ordinary = Find-ByName $root "Preview pane" }
    if ($null -ne $ordinary) { throw "ordinary preview pane is visible beside the column preview" }
    $photo = Find-ListItem $root '(^| )photo\.jpg( |$)' $surface
    if ($null -eq $photo) {
        $rows = New-Object System.Collections.Generic.List[string]
        $items = $root.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            (New-Object Windows.Automation.PropertyCondition(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::ListItem)))
        $surfaceRect = $surface.Current.BoundingRectangle
        $observations.Add("surface bounds: $($surfaceRect.Left),$($surfaceRect.Top) $($surfaceRect.Width)x$($surfaceRect.Height)")
        foreach ($item in $items) {
            $rect = $item.Current.BoundingRectangle
            $rows.Add("$($item.Current.Name) [$($rect.Left),$($rect.Top) $($rect.Width)x$($rect.Height)]")
            if ($rows.Count -ge 12) { break }
        }
        $observations.Add(("list items: " + ($rows -join " | ")))
        throw "photo.jpg disappeared after switching to Columns"
    }
    $observations.Add(("photo click: " + (Click-Row $photo)))
    $image = $null
    $imageDeadline = [DateTime]::UtcNow.AddSeconds(12)
    do {
        $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
        $image = Find-ByName $root "Preview image loaded"
        if ($null -ne $image -and $image.Current.BoundingRectangle.Width -gt 8 -and $image.Current.BoundingRectangle.Height -gt 8) { break }
        $image = $null
        Start-Sleep -Milliseconds 300
    } while ([DateTime]::UtcNow -lt $imageDeadline)
    if ($null -eq $image) {
        $names = New-Object System.Collections.Generic.List[string]
        $all = $root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)
        foreach ($element in $all) {
            $name = [string]$element.Current.Name
            if ($name.IndexOf("Preview") -ge 0 -or $name.IndexOf("photo") -ge 0 -or $name.IndexOf("Integrated") -ge 0) {
                $rect = $element.Current.BoundingRectangle
                $names.Add("$name [$($rect.Width)x$($rect.Height)]")
            }
        }
        $observations.Add(("preview uia: " + ($names -join " | ")))
        throw "integrated JPEG preview did not expose a sized Preview image loaded element"
    }
    $imageBounds = $image.Current.BoundingRectangle
    $preview = Find-ByName $root ([string]::new(@([char]0x6574, [char]0x5408, [char]0x9810, [char]0x89BD)))
    if ($null -eq $preview) { $preview = Find-ByName $root "Integrated preview" }
    if ($null -eq $preview) { throw "integrated preview slot was not exposed to UIA" }
    $previewBounds = $preview.Current.BoundingRectangle
    $observations.Add("image bounds: $($imageBounds.Left),$($imageBounds.Top) $($imageBounds.Width)x$($imageBounds.Height)")
    $observations.Add("preview bounds: $($previewBounds.Left),$($previewBounds.Top) $($previewBounds.Width)x$($previewBounds.Height)")
    if ($imageBounds.Left -lt ($previewBounds.Left - 2) -or
        ($imageBounds.Left + $imageBounds.Width) -gt ($previewBounds.Right + 2) -or
        $imageBounds.Top -lt ($previewBounds.Top - 2) -or
        ($imageBounds.Top + $imageBounds.Height) -gt ($previewBounds.Bottom + 2)) {
        throw "JPEG preview is not inside the integrated preview slot"
    }
    $selectedCapture = Join-Path $OutputDirectory "jpeg-preview-selected.png"
    Save-Window $process.MainWindowHandle $selectedCapture
    if (-not (Test-Path -LiteralPath $selectedCapture)) {
        throw "selected JPEG screenshot was not captured"
    }
    $captureBitmap = New-Object System.Drawing.Bitmap $selectedCapture
    try {
        $centerX = [int]($imageBounds.Left + $imageBounds.Width / 2)
        $centerY = [int]($imageBounds.Top + $imageBounds.Height / 2)
        $pixel = $captureBitmap.GetPixel($centerX, $centerY)
        $observations.Add("jpeg center pixel: R=$($pixel.R) G=$($pixel.G) B=$($pixel.B)")
        if ($pixel.R -lt 150 -or $pixel.G -gt 100 -or $pixel.B -gt 150) {
            throw "integrated preview did not draw the runner JPEG's red pixels"
        }
    } finally {
        $captureBitmap.Dispose()
    }
    $notes = Find-ListItem $root '(^| )notes\.txt( |$)' $surface
    if ($null -eq $notes) { throw "notes.txt was not listed for the stale-preview check" }
    $observations.Add(("notes click: " + (Click-Row $notes)))
    $staleDeadline = [DateTime]::UtcNow.AddSeconds(8)
    $stale = Find-ByName $root "Preview image loaded"
    do {
        Start-Sleep -Milliseconds 250
        $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
        $stale = Find-ByName $root "Preview image loaded"
        if ($null -eq $stale) { break }
    } while ([DateTime]::UtcNow -lt $staleDeadline)
    if ($null -ne $stale) { throw "JPEG preview remained after selecting notes.txt" }
    $observations.Add("stale jpeg preview cleared")
    Save-Window $process.MainWindowHandle (Join-Path $OutputDirectory "jpeg-preview.png")
    Write-Report "PASS" "Columns mode showed the runner JPEG inside the integrated preview and cleared it after the selection changed." $observations
    exit 0
} catch {
    if ($process -and $process.MainWindowHandle -ne [IntPtr]::Zero) {
        Save-Window $process.MainWindowHandle (Join-Path $OutputDirectory "jpeg-preview-failed.png")
    }
    $observations.Add($_.Exception.Message)
    Write-Report "FAIL" $_.Exception.Message $observations
    exit 1
} finally {
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    }
}
