$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$document = Get-Content -LiteralPath (Join-Path $repo 'sdk\NATIVE_PLUGIN_OPERATIONS.md') -Raw -Encoding UTF8
$readme = Get-Content -LiteralPath (Join-Path $repo 'sdk\README.md') -Raw -Encoding UTF8
$diagnostics = Get-Content -LiteralPath (Join-Path $repo 'sdk\PLUGIN_DIAGNOSTICS.md') -Raw -Encoding UTF8
$lifecycle = Get-Content -LiteralPath (Join-Path $repo 'sdk\PACKAGE_LIFECYCLE.md') -Raw -Encoding UTF8
$traditionalChinese = [string]::new([char[]](0x7e41, 0x9ad4, 0x4e2d, 0x6587))

foreach ($requiredText in @(
    'Native Rust plugin operations and safety guide',
    '## English',
    $traditionalChinese,
    'zh-TW',
    'in the SuperExplorer process',
    'current-user authority',
    'Manifest capabilities constrain host-provided APIs',
    'not a security boundary',
    'no hot load, hot update',
    'Windows x64 MSVC',
    'operation currently attributable',
    'startup only',
    'resident until the process exits',
    'hot-unload',
    'DisabledResident',
    'NativeDispatchLeaseV1',
    '| Startup | validate package',
    '| Disable loaded feature | close gate',
    '| Remove loaded | close and drain',
    '| Shutdown | close all gates',
    'NativeRestartReasonV1',
    'UnloadedEnable', 'Install', 'Update', 'Replace', 'Remove', 'DrainTimedOut', 'StartupAborted',
    'std::process::abort',
    'Clear failure', 'MarkerFailure', 'faults activation', 'global denial',
    'NativeSafeModeIncidentV1', 'RegistrarInProgress', 'UnsafeMarkerState',
    'MarkerStateUnavailable',
    'confirm_safe_mode_incident', 'safe_mode_denies_all',
    'Confirmation acknowledges a recovery decision',
    'same native code again',
    'heuristic record',
    'unknown or expired incident ID',
    'native_call_timings', '128 V1',
    'performance SLA',
    'Accepted', 'PluginError', 'Incompatible', 'Panicked', 'MarkerFailure', 'SafeModeDenied',
    'May be shown or logged / allow', 'Must be redacted / deny',
    'plugin DLL', 'user absolute path', 'marker path/content',
    'raw OS error', 'panic payload/backtrace',
    'identifiers, not',
    'native-call-guard-contract.ps1'
)) {
    if (-not $document.Contains($requiredText)) {
        throw "native plugin security document is missing: $requiredText"
    }
}

function ConvertFrom-Utf8Base64([string] $Value) {
    [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($Value))
}
$requiredTraditionalText = @(
    '5YWx55So5L2N5Z2A56m66ZaT';
    'Y2FwYWJpbGl0eSDlj6rpmZDliLYgaG9zdCDmj5DkvpvnmoQgQVBJ';
    '5LiN5piv5a6J5YWo6IOM5pu4';
    '55uu5YmNIGR1cmFibGUgbWFya2VyIOWPquS/neittyByZWdpc3RyYXI=';
    'RExMIOS4gOaXpiBtYXA=';
    'c3RhcnR1cCDnmoTlm7rlrprpoIbluo8=';
    'Y2xvc2UgZ2F0ZQ==';
    'ZGV0YWNoIGNvbnRyaWJ1dGlvbg==';
    'Y2FuY2VsIGhvc3QtbWFuYWdlZCB3b3Jr';
    'bG9hZGVkIHJlbW92ZSDmnIDntYLmmK8=';
    'UGVuZGluZ1Jlc3RhcnQoUmVtb3ZlKQ==';
    'dW5kZWZpbmVkIGJlaGF2aW9y';
    'bWVtb3J5IGNvcnJ1cHRpb24=';
    'aW5maW5pdGUgbG9vcA==';
    'YWxsb2NhdG9y77yPR1BVSSBzdGF0ZQ==';
    'TmF0aXZlTG9hZGVyRGlhZ25vc3RpY0NvZGVWMQ==';
    'TmF0aXZlUmVzdGFydFJlYXNvblYx';
    '5Y+q5pyJIG1hcmtlcg==';
    'Y2xlYXIg5aSx5pWX5pyD6KiY6YyE';
    'TWFya2VyU3RhdGVVbmF2YWlsYWJsZQ=='
) | ForEach-Object { ConvertFrom-Utf8Base64 $_ }
foreach ($requiredText in $requiredTraditionalText) {
    if (-not $document.Contains($requiredText)) {
        throw "traditional Chinese native security section is missing: $requiredText"
    }
}

foreach ($forbiddenText in @(
    'Safe Mode is a security boundary',
    'provides a sandbox',
    'hot-unload is supported',
    'force-unload the DLL',
    'delete marker files to bypass',
    'all native failures produce a Safe Mode incident',
    'confirmation proves the plugin is safe'
)) {
    if ($document.Contains($forbiddenText)) {
        throw "native plugin security document makes an unsafe promise: $forbiddenText"
    }
}

foreach ($badEncoding in @(
    [string][char]0xfffd,
    [string][char]0x00c3,
    [string][char]0x00c2,
    ([string][char]0x00e2 + [string][char]0x20ac)
)) {
    if ($document.Contains($badEncoding)) {
        throw 'native plugin security document contains replacement-character or mojibake evidence'
    }
}

foreach ($crossLink in @($readme, $diagnostics, $lifecycle)) {
    if (-not $crossLink.Contains('NATIVE_PLUGIN_OPERATIONS.md')) {
        throw 'SDK documentation is missing the native plugin operations cross-link'
    }
}
if ($lifecycle.Contains('a future') -or $lifecycle.Contains('separate later host stages')) {
    throw 'package lifecycle document still describes implemented native lifecycle work as future/later'
}

Write-Output 'native plugin security documentation contract: PASS'
