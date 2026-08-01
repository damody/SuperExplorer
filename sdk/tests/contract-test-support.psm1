Set-StrictMode -Version Latest
function Get-ToolField { param([string]$Text,[string]$Name); $m=[regex]::Match($Text,"(?m)^$([regex]::Escape($Name)):\s*(\S+)\s*$"); if(!$m.Success){throw "Tool output is missing '$Name'."}; $m.Groups[1].Value }
function Convert-ToolchainManifest { param([string]$Text,[string]$Name); $read={param([string]$Key);$m=[regex]::Match($Text,"(?m)^\s*$Key\s*=\s*(.+?)\s*$");if(!$m.Success){throw "$Name is missing '$Key'."};$m.Groups[1].Value.Trim()};$list={param([string]$Value);if($Value -notmatch '^\[(.*)\]$'){throw "$Name has invalid list '$Value'."};@($Matches[1]-split ','|%{$_.Trim().Trim('"',"'")}|?{$_})};[pscustomobject]@{Channel=(& $read channel).Trim('"',"'");Profile=(& $read profile).Trim('"',"'");Targets=@(& $list (& $read targets));Components=@(& $list (& $read components))} }
function Assert-ToolchainContract {
 param([Parameter(Mandatory)][string]$RustcOutput,[Parameter(Mandatory)][string]$CargoOutput,[Parameter(Mandatory)][string]$RustupActiveOutput,[Parameter(Mandatory)][string]$InstalledTargetsOutput,[Parameter(Mandatory)][string]$RootToolchainText,[Parameter(Mandatory)][string]$SdkToolchainText)
 $v='1.97.1';$t='x86_64-pc-windows-msvc';$rc='8bab26f4f68e0e26f0bb7960be334d5b520ea452';$cc='c980f4866141969fab6254a680546a277789d6f0'
 $checks=@(@{N='rust release';A=(Get-ToolField $RustcOutput release);E=$v},@{N='cargo release';A=(Get-ToolField $CargoOutput release);E=$v},@{N='rustc commit';A=(Get-ToolField $RustcOutput 'commit-hash');E=$rc},@{N='cargo commit';A=(Get-ToolField $CargoOutput 'commit-hash');E=$cc},@{N='rustc host';A=(Get-ToolField $RustcOutput host);E=$t},@{N='cargo host';A=(Get-ToolField $CargoOutput host);E=$t})
 $active=(($RustupActiveOutput-split '\r?\n'|?{$_-match'\S'}|select -First 1).Trim() -replace '\s+\(.*\)$','');$checks+=@{N='rustup active toolchain';A=$active;E="$v-$t"}
 $installed=@($InstalledTargetsOutput-split '\r?\n'|%{($_-split'\s+')[0]}|?{$_});if($t-notin$installed){throw "Toolchain contract mismatch: required target '$t' is not installed."}
 $root=Convert-ToolchainManifest $RootToolchainText 'root rust-toolchain.toml';$sdk=Convert-ToolchainManifest $SdkToolchainText 'sdk/rust-toolchain.toml'
 if($root.Channel-ne$sdk.Channel-or$root.Profile-ne$sdk.Profile){throw 'Toolchain contract mismatch: root/sdk channel or profile differ.'};foreach($f in 'Targets','Components'){if(($sdk.$f-join',')-ne($root.$f-join',')){throw "Toolchain contract mismatch: root/sdk $f differ."}}
 if($sdk.Channel-ne$v-or$sdk.Profile-ne'minimal'-or($sdk.Targets-join',')-ne$t-or($sdk.Components-join',')-ne'rustfmt,clippy'){throw 'Toolchain contract mismatch: manifest baseline is incorrect.'}
 $bad=@($checks|?{$_.A-ne$_.E});if($bad.Count){throw(($bad|%{"$($_.N): expected '$($_.E)', got '$($_.A)'"})-join'; ')};[pscustomobject]@{Status='ok';RustVersion=$v;Target=$t;RustcCommit=$rc;CargoCommit=$cc}
}
Export-ModuleMember -Function Assert-ToolchainContract,Convert-ToolchainManifest,Get-ToolField
