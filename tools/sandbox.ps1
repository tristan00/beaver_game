param(
    [ValidateSet('telemark', 'algonquin', 'bavarian', 'voyageurs')]
    [string]$Map = 'telemark',
    [string]$Script = '',
    [string]$Camera = '',
    [switch]$NoTrees
)

$repo = Split-Path -Parent $PSScriptRoot
if ($Script) { $Script = (Resolve-Path -LiteralPath $Script -ErrorAction Stop).Path }
$savedCamera = $env:BC_CAM_AT
$savedTrees = $env:BC_NO_TREES
try {
    if ($Camera) { $env:BC_CAM_AT = $Camera } else { Remove-Item Env:BC_CAM_AT -ErrorAction SilentlyContinue }
    if ($NoTrees) { $env:BC_NO_TREES = '1' } else { Remove-Item Env:BC_NO_TREES -ErrorAction SilentlyContinue }
    Push-Location -LiteralPath $repo
    $gameArgs = if ($Script) { @('--sandbox-script', $Script) } else { @('--sandbox', '--map', $Map) }
    & cargo run --release -p game --bin beaver_colony -- @gameArgs
} finally {
    Pop-Location
    $env:BC_CAM_AT = $savedCamera
    $env:BC_NO_TREES = $savedTrees
}
