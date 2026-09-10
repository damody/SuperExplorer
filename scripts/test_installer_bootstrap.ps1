$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$sourceBat = Join-Path $workspace 'build_test_install.bat'
$sourceLuaDir = Join-Path $workspace 'build\tools\lua'
$fixtureRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("superexplorer-bootstrap-" + [guid]::NewGuid().ToString('N'))

function Invoke-FixtureBat {
    param(
        [Parameter(Mandatory = $true)] [string]$FixtureRoot,
        [string[]]$ArgumentList = @(),
        [switch]$ExpectDetach
    )
    $markerPath = Join-Path $FixtureRoot 'worker-marker.txt'
    $logPath = Join-Path $FixtureRoot 'worker-log.txt'
    Remove-Item -LiteralPath $markerPath, $logPath -ErrorAction SilentlyContinue
    $info = New-Object System.Diagnostics.ProcessStartInfo
    $info.FileName = Join-Path $FixtureRoot 'build_test_install.bat'
    $info.Arguments = ($ArgumentList -join ' ')
    $info.WorkingDirectory = $FixtureRoot
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $process = [Diagnostics.Process]::Start($info)
    if (-not $process.WaitForExit(20000)) {
        $process.Kill()
        throw 'fixture bat did not exit within 20s'
    }
    if ($ExpectDetach) {
        if ($process.ExitCode -ne 0) { throw "detached parent exit $($process.ExitCode)" }
        if (Test-Path -LiteralPath $markerPath) { throw 'detached parent waited for the worker lua' }
        $deadline = [DateTime]::UtcNow.AddSeconds(20)
        while ([DateTime]::UtcNow -lt $deadline) {
            if (Test-Path -LiteralPath $markerPath) { break }
            Start-Sleep -Milliseconds 100
        }
        if (-not (Test-Path -LiteralPath $markerPath)) {
            $log = if (Test-Path -LiteralPath $logPath) { Get-Content -LiteralPath $logPath -Raw } else { '<missing worker-log.txt>' }
            throw "detached worker did not write the marker; log=$log"
        }
    } else {
        if ($process.ExitCode -ne 0) { throw "in-process bat exit $($process.ExitCode)" }
        if (-not (Test-Path -LiteralPath $markerPath)) { throw 'in-process worker did not write the marker before parent exit' }
    }
    return Get-Content -LiteralPath $markerPath -Raw
}

try {
    New-Item -ItemType Directory -Path (Join-Path $fixtureRoot 'build\tools') | Out-Null
    Copy-Item -LiteralPath $sourceBat -Destination (Join-Path $fixtureRoot 'build_test_install.bat')
    Copy-Item -LiteralPath $sourceLuaDir -Destination (Join-Path $fixtureRoot 'build\tools\lua') -Recurse
    Set-Content -LiteralPath (Join-Path $fixtureRoot 'build\build_install.lua') -Encoding ASCII -Value @'
local root = assert(arg[0]:match("^(.*)[\\/]build[\\/]build_install%.lua$"), "missing script path")
local log = assert(io.open(root .. "\\worker-log.txt", "wb"))
log:write("arg0=" .. tostring(arg[0]) .. "\n")
for i = 1, #arg do log:write("arg" .. i .. "=" .. tostring(arg[i]) .. "\n") end
assert(log:close())
local marker = assert(io.open(root .. "\\worker-marker.txt", "wb"))
marker:write(table.concat(arg, "\n"))
assert(marker:close())
os.exit(0)
'@

    $detachArgs = Invoke-FixtureBat -FixtureRoot $fixtureRoot -ExpectDetach
    if ($detachArgs -notmatch '--component' -or $detachArgs -notmatch '--auto-install') {
        throw "detached worker lost installer arguments: $detachArgs"
    }

    Invoke-FixtureBat -FixtureRoot $fixtureRoot -ArgumentList @('--check') | Out-Null
    Invoke-FixtureBat -FixtureRoot $fixtureRoot -ArgumentList @('--no-launch') | Out-Null

    Write-Output 'Installer bootstrap detach behavior PASS'
} finally {
    Get-CimInstance Win32_Process -Filter "Name='cmd.exe'" | Where-Object {
        $_.CommandLine -and $_.CommandLine.Contains('superexplorer-bootstrap-')
    } | ForEach-Object {
        Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path -LiteralPath $fixtureRoot) {
        Remove-Item -LiteralPath $fixtureRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
