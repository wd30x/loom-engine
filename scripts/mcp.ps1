#Requires -Version 7
# Entry point of the `loom` MCP server (.mcp.json; docs/architecture.md
# section 13). Builds loom and loom-mcp, copies both to a fresh directory
# under target/mcp-run/ and runs loom-mcp from there, so a running MCP
# session never locks target/debug/ against the next build.
#
# stdout belongs to the MCP transport: everything this script says goes to
# stderr.

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root   # relative tool paths are relative to the repository root

# cargo writes to stderr; anything it might print on stdout goes there too.
cargo build -q -p loom_engine -p loom_mcp | ForEach-Object { [Console]::Error.WriteLine($_) }
if ($LASTEXITCODE -ne 0) {
    [Console]::Error.WriteLine("mcp.ps1: cargo build failed with exit code $LASTEXITCODE")
    exit 1
}

$suffix = if ($IsWindows) { '.exe' } else { '' }
$runs = Join-Path $root 'target/mcp-run'
$name = '{0:yyyyMMdd-HHmmss-fff}-{1}' -f (Get-Date), $PID
$dir = Join-Path $runs $name
$null = New-Item -ItemType Directory -Force $dir
foreach ($exe in "loom$suffix", "loom-mcp$suffix") {
    Copy-Item (Join-Path $root "target/debug/$exe") $dir
}

# Remove the directories of sessions that have ended. A running session's
# loom-mcp cannot be deleted: skip its directory, and only then touch the
# rest, since its loom is only started (and locked) on the first tool call.
foreach ($old in Get-ChildItem $runs -Directory | Where-Object Name -ne $name) {
    $mcp = Join-Path $old.FullName "loom-mcp$suffix"
    if (Test-Path $mcp) {
        try { Remove-Item $mcp -Force -ErrorAction Stop } catch { continue }
    }
    Remove-Item $old.FullName -Recurse -Force -ErrorAction SilentlyContinue
}

# Not through a pipeline: loom-mcp gets this script's stdin and stdout as they are.
& (Join-Path $dir "loom-mcp$suffix")
exit $LASTEXITCODE
