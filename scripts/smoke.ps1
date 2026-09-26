#Requires -Version 7
# Smoke test on real hardware: GPU rendering, window exit paths, keyboard
# record/replay and minimized CPU use. Needs a GPU; the W group also needs a
# desktop session. Not part of verify.ps1.
#
# Usage: pwsh scripts/smoke.ps1 [-HeadlessOnly]
# Prints one PASS/FAIL line per check, then "smoke: N passed, M failed".
# Exits 1 if anything failed.

param([switch]$HeadlessOnly)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root
$clock = [Diagnostics.Stopwatch]::StartNew()

cargo build -p loom_engine
if ($LASTEXITCODE -ne 0) { Write-Host 'smoke: build failed'; exit 1 }
$exe = Join-Path $root 'target/debug/loom.exe'

$out = 'out/smoke'
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Force $out | Out-Null

if (-not ('SmokeWin32' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;

public static class SmokeWin32 {
    public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int max);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd, int cmd);

    // The visible top-level window of `pid` titled `title`. Matching on the
    // title matters: winit also creates a hidden helper window.
    public static IntPtr FindWindow(uint pid, string title) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((hwnd, _) => {
            uint owner;
            GetWindowThreadProcessId(hwnd, out owner);
            if (owner != pid || !IsWindowVisible(hwnd)) return true;
            var text = new StringBuilder(256);
            GetWindowText(hwnd, text, text.Capacity);
            if (text.ToString() != title) return true;
            found = hwnd;
            return false;
        }, IntPtr.Zero);
        return found;
    }
}
'@
}

$WM_KEYDOWN = 0x0100; $WM_KEYUP = 0x0101; $WM_CLOSE = 0x0010
$SW_MINIMIZE = 6; $SW_RESTORE = 9
# Virtual-key code and scan code; winit reads the physical key from the scan code.
$keyCodes = @{ W = @(0x57, 0x11); A = @(0x41, 0x1E); D = @(0x44, 0x20); Esc = @(0x1B, 0x01) }
# opengl32.dll is not listed: like dxgi.dll it is a static import of loom.exe
# (wgpu's GL backend links it), so it is always loaded. It is only the GL
# loader; the actual GL driver DLL is caught by the vendor prefixes below.
$gpuDlls = @('vulkan-1.dll', 'd3d12.dll', 'd3d12core.dll', 'd3d11.dll')
$gpuDriverPrefixes = @('nv', 'amd', 'ig')

$script:passed = 0
$script:failed = 0

# Runs one check. $body throws to fail; its message becomes the reason.
# Measurements the body stores in $script:detail are appended to the line.
function Test-Check([string]$id, [string]$desc, [scriptblock]$body) {
    $script:detail = ''
    try {
        $null = & $body
        $script:passed++
        Write-Host "PASS $id $desc$(if ($script:detail) { " ($script:detail)" })"
    } catch {
        $script:failed++
        Write-Host "FAIL $id ${desc}: $($_.Exception.Message)$(if ($script:detail) { " ($script:detail)" })"
    }
}

function Assert-True([bool]$condition, [string]$message) {
    if (-not $condition) { throw $message }
}

# Runs loom to completion. Returns exit code and the parsed stdout JSON.
function Invoke-Loom([string[]]$loomArgs) {
    $stdout = & $exe @loomArgs 2>$null
    $code = $LASTEXITCODE
    $json = if ($stdout) { $stdout | ConvertFrom-Json } else { $null }
    [pscustomobject]@{ Code = $code; Json = $json }
}

function Get-PngSize([string]$path) {
    $bytes = [IO.File]::ReadAllBytes($path)
    $w = ([int]$bytes[16] -shl 24) -bor ([int]$bytes[17] -shl 16) -bor ([int]$bytes[18] -shl 8) -bor $bytes[19]
    $h = ([int]$bytes[20] -shl 24) -bor ([int]$bytes[21] -shl 16) -bor ([int]$bytes[22] -shl 8) -bor $bytes[23]
    "${w}x${h}"
}

function Get-ModuleNames($process) {
    $process.Refresh()
    @($process.Modules | ForEach-Object { $_.ModuleName.ToLowerInvariant() })
}

function Test-GpuModule([string]$name) {
    ($gpuDlls -contains $name) -or @($gpuDriverPrefixes | Where-Object { $name.StartsWith($_) }).Count -gt 0
}

# Starts `loom run <args>` in the background with stdout/stderr in files.
function Start-Loom([string]$name, [string[]]$loomArgs) {
    $stdout = Join-Path $out "$name.stdout.txt"
    $stderr = Join-Path $out "$name.stderr.txt"
    $process = Start-Process $exe -ArgumentList (@('run') + $loomArgs) -PassThru -NoNewWindow `
        -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    $null = $process.Handle   # keep a handle so ExitCode stays readable after exit
    [pscustomobject]@{ Process = $process; Stdout = $stdout; Stderr = $stderr; Window = [IntPtr]::Zero }
}

# Starts a windowed run and waits (up to 10 s) for its window.
function Start-LoomWindow([string]$name, [string[]]$loomArgs) {
    $run = Start-Loom $name $loomArgs
    $deadline = (Get-Date).AddSeconds(10)
    while ((Get-Date) -lt $deadline) {
        $run.Window = [SmokeWin32]::FindWindow([uint32]$run.Process.Id, 'loom')
        if ($run.Window -ne [IntPtr]::Zero) { return $run }
        if ($run.Process.HasExited) { throw "loom exited early with code $($run.Process.ExitCode)" }
        Start-Sleep -Milliseconds 100
    }
    Stop-Loom $run
    throw 'loom window did not appear within 10 s'
}

# Kills the process if it is still running.
function Stop-Loom($run) {
    if ($run -and -not $run.Process.HasExited) { $run.Process.Kill(); $run.Process.WaitForExit() }
}

# Waits (up to 10 s) for a run to exit. Returns exit code and parsed JSON.
function Wait-Loom($run) {
    if (-not $run.Process.WaitForExit(10000)) {
        Stop-Loom $run
        throw 'loom did not exit within 10 s'
    }
    $text = Get-Content $run.Stdout -Raw
    $json = if ($text) { $text | ConvertFrom-Json } else { $null }
    [pscustomobject]@{ Code = $run.Process.ExitCode; Json = $json; Stderr = $run.Stderr }
}

function Send-Key($run, [string]$key, [bool]$down) {
    $vk, $scan = $keyCodes[$key]
    $lParam = [int64](1 -bor ($scan -shl 16))
    if ($down) {
        [void][SmokeWin32]::PostMessage($run.Window, $WM_KEYDOWN, [IntPtr]$vk, [IntPtr]$lParam)
    } else {
        [void][SmokeWin32]::PostMessage($run.Window, $WM_KEYUP, [IntPtr]$vk, [IntPtr]($lParam -bor 0xC0000000L))
    }
}

function Send-Close($run) { [void][SmokeWin32]::PostMessage($run.Window, $WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) }
function Set-WindowState($run, [int]$cmd) { [void][SmokeWin32]::ShowWindow($run.Window, $cmd) }

function Assert-ExitOk($result) {
    if ($result.Code -ne 0) {
        $tail = if ($result.PSObject.Properties['Stderr'] -and (Test-Path $result.Stderr)) {
            (Get-Content $result.Stderr -Tail 3) -join ' | '
        }
        throw "exit code $($result.Code), stdout: $($result.Json | ConvertTo-Json -Compress), stderr tail: $tail"
    }
    Assert-True ($null -ne $result.Json -and $result.Json.status -eq 'ok') 'status is not ok'
}

# Headless replay of `script` for `ticks` ticks; returns the state file path.
function Invoke-HeadlessReplay([string]$script, [int64]$ticks, [string]$state) {
    $r = Invoke-Loom @('run', '--headless', '--frames', "$ticks", '--input', $script, '--dump-state', $state)
    Assert-ExitOk $r
    $state
}

function Assert-SameBytes([string]$a, [string]$b) {
    $ha = (Get-FileHash $a).Hash; $hb = (Get-FileHash $b).Hash
    Assert-True ($ha -eq $hb) "$a ($ha) differs from $b ($hb)"
}

# ---------------------------------------------------------------- H: headless

Test-Check 'H1' 'headless screenshots are byte-identical and 1280x720' {
    foreach ($n in 'a', 'b') {
        $r = Invoke-Loom @('run', '--headless', '--frames', '60', '--screenshot', "$out/h1_$n.png")
        Assert-ExitOk $r
    }
    Assert-SameBytes "$out/h1_a.png" "$out/h1_b.png"
    $size = Get-PngSize "$out/h1_a.png"
    Assert-True ($size -eq '1280x720') "PNG is $size"
}

Test-Check 'H2' '--size 640x360 gives a 640x360 PNG' {
    $r = Invoke-Loom @('run', '--headless', '--frames', '60', '--size', '640x360', '--screenshot', "$out/h2.png")
    Assert-ExitOk $r
    $size = Get-PngSize "$out/h2.png"
    Assert-True ($size -eq '640x360') "PNG is $size"
}

Test-Check 'H3' 'headless run without screenshot loads no GPU driver' {
    $run = Start-Loom 'h3' @('--headless', '--frames', '4000000000')
    try {
        Start-Sleep -Milliseconds 1500
        Assert-True (-not $run.Process.HasExited) 'the long run exited early'
        $gpu = @(Get-ModuleNames $run.Process | Where-Object { Test-GpuModule $_ })
        Assert-True ($gpu.Count -eq 0) "loaded $($gpu -join ', ')"
    } finally {
        Stop-Loom $run
    }
}

# ---------------------------------------------------------------- W: window

if (-not $HeadlessOnly) {
    Test-Check 'W0' 'window run loads a GPU driver (control for H3)' {
        $run = Start-LoomWindow 'w0' @()
        try {
            Start-Sleep -Milliseconds 1500
            $modules = Get-ModuleNames $run.Process
            Assert-True (($modules -contains 'vulkan-1.dll') -or ($modules -contains 'd3d12.dll')) 'neither vulkan-1.dll nor d3d12.dll is loaded'
            Send-Close $run
            Assert-ExitOk (Wait-Loom $run)
        } finally {
            Stop-Loom $run
        }
    }

    Test-Check 'W1' 'Esc exits 0 and saves the screenshot' {
        $run = Start-LoomWindow 'w1' @('--screenshot', "$out/w1.png")
        try {
            Start-Sleep -Milliseconds 1000
            Send-Key $run 'Esc' $true
            $r = Wait-Loom $run
            Assert-ExitOk $r
            Assert-True ($null -ne $r.Json.screenshot) 'screenshot is null'
            Assert-True (Test-Path "$out/w1.png") 'PNG missing'
        } finally {
            Stop-Loom $run
        }
    }

    Test-Check 'W2' 'closing the window exits 0 and saves the screenshot' {
        $run = Start-LoomWindow 'w2' @('--screenshot', "$out/w2.png")
        try {
            Start-Sleep -Milliseconds 1000
            Send-Close $run
            $r = Wait-Loom $run
            Assert-ExitOk $r
            Assert-True ($null -ne $r.Json.screenshot) 'screenshot is null'
            Assert-True (Test-Path "$out/w2.png") 'PNG missing'
        } finally {
            Stop-Loom $run
        }
    }

    Test-Check 'W3' 'closing while minimized skips the screenshot but dumps the state' {
        $run = Start-LoomWindow 'w3' @('--screenshot', "$out/w3.png", '--dump-state', "$out/w3.json")
        try {
            Start-Sleep -Milliseconds 1000
            Set-WindowState $run $SW_MINIMIZE
            Start-Sleep -Milliseconds 1000
            Send-Close $run
            $r = Wait-Loom $run
            Assert-ExitOk $r
            Assert-True ($null -eq $r.Json.screenshot) 'screenshot is not null'
            Assert-True (-not (Test-Path "$out/w3.png")) 'PNG was written'
            Assert-True (Test-Path "$out/w3.json") 'state file missing'
        } finally {
            Stop-Loom $run
        }
    }

    Test-Check 'W4' 'rendering resumes after minimize and restore' {
        $run = Start-LoomWindow 'w4' @('--screenshot', "$out/w4.png")
        try {
            Start-Sleep -Milliseconds 1000
            Set-WindowState $run $SW_MINIMIZE
            Start-Sleep -Milliseconds 1000
            Set-WindowState $run $SW_RESTORE
            Start-Sleep -Milliseconds 1000
            Send-Key $run 'Esc' $true
            $r = Wait-Loom $run
            Assert-ExitOk $r
            Assert-True ($null -ne $r.Json.screenshot) 'screenshot is null: no frame rendered after restore'
        } finally {
            Stop-Loom $run
        }
    }

    Test-Check 'W5' 'keyboard recording replays headless to identical state' {
        $run = Start-LoomWindow 'w5' @('--record', "$out/w5.ron", '--dump-state', "$out/w5.json")
        try {
            Start-Sleep -Milliseconds 700
            Send-Key $run 'D' $true; Start-Sleep -Milliseconds 1000
            Send-Key $run 'W' $true; Start-Sleep -Milliseconds 500
            Send-Key $run 'W' $false; Send-Key $run 'D' $false; Start-Sleep -Milliseconds 300
            foreach ($i in 1..3) {
                Send-Key $run 'A' $true; Send-Key $run 'A' $false
                Start-Sleep -Milliseconds 250
            }
            Start-Sleep -Milliseconds 300
            Send-Key $run 'Esc' $true
            Assert-ExitOk (Wait-Loom $run)
        } finally {
            Stop-Loom $run
        }
        $tick = (Get-Content "$out/w5.json" -Raw | ConvertFrom-Json).tick
        $replay = Invoke-HeadlessReplay "$out/w5.ron" $tick "$out/w5_replay.json"
        Assert-SameBytes "$out/w5.json" $replay

        $left = @([regex]::Matches((Get-Content "$out/w5.ron" -Raw),
                'tick: (\d+), action: "move_left", down: (true|false)') |
            ForEach-Object { [pscustomobject]@{ Tick = [int64]$_.Groups[1].Value; Down = $_.Groups[2].Value -eq 'true' } })
        Assert-True ($left.Count -eq 6) "expected 3 move_left press/release pairs, found $($left.Count) events"
        for ($i = 0; $i -lt 6; $i += 2) {
            Assert-True ($left[$i].Down -and -not $left[$i + 1].Down) "move_left events are not press/release pairs"
            Assert-True ($left[$i + 1].Tick -gt $left[$i].Tick) "tap $($i / 2 + 1) pressed and released in tick $($left[$i].Tick)"
        }
    }

    Test-Check 'W6' 'window replay matches headless replay' {
        Assert-True (Test-Path "$out/w5.ron") 'no recording from W5'
        $run = Start-LoomWindow 'w6' @('--input', "$out/w5.ron", '--dump-state', "$out/w6.json")
        try {
            Start-Sleep -Milliseconds 1500
            Send-Key $run 'Esc' $true
            Assert-ExitOk (Wait-Loom $run)
        } finally {
            Stop-Loom $run
        }
        $tick = (Get-Content "$out/w6.json" -Raw | ConvertFrom-Json).tick
        $replay = Invoke-HeadlessReplay "$out/w5.ron" $tick "$out/w6_replay.json"
        Assert-SameBytes "$out/w6.json" $replay
    }

    Test-Check 'W7' 'minimized window stays idle while ticks follow the wall clock' {
        $run = Start-LoomWindow 'w7' @('--dump-state', "$out/w7.json")
        try {
            Start-Sleep -Milliseconds 1000
            Set-WindowState $run $SW_MINIMIZE
            Start-Sleep -Milliseconds 200   # let the minimize take effect
            $run.Process.Refresh()
            $cpu0 = $run.Process.TotalProcessorTime
            $wall = [Diagnostics.Stopwatch]::StartNew()
            Start-Sleep -Milliseconds 3000
            $run.Process.Refresh()
            $cpu = ($run.Process.TotalProcessorTime - $cpu0).TotalSeconds / $wall.Elapsed.TotalSeconds
            Send-Close $run
            $r = Wait-Loom $run
            Assert-ExitOk $r
            $lifetime = ($run.Process.ExitTime - $run.Process.StartTime).TotalSeconds
            $ratio = $r.Json.ticks / ($lifetime * 60)
            # Wall time not covered by ticks. Startup (GPU init) and shutdown
            # cost a fixed ~0.9 s; had ticks stopped while minimized, the
            # 3 s minimized span would add to it.
            $untracked = $lifetime - $r.Json.ticks / 60
            $script:detail = 'minimized CPU {0:P1} of one core; {1} ticks in {2:N2} s wall = {3:N3} x 60 Hz; {4:N2} s not covered by ticks' -f `
                $cpu, $r.Json.ticks, $lifetime, $ratio, $untracked
            Assert-True ($cpu -lt 0.05) ('minimized CPU is {0:P1} of one core' -f $cpu)
            Assert-True ($untracked -ge 0 -and $untracked -lt 1.5) ('{0:N2} s of wall time not covered by ticks' -f $untracked)
        } finally {
            Stop-Loom $run
        }
    }
}

Write-Host ('elapsed {0:N1} s' -f $clock.Elapsed.TotalSeconds)
Write-Host "smoke: $script:passed passed, $script:failed failed"
if ($script:failed -gt 0) { exit 1 }
