[CmdletBinding()]
param(
    [string]$Distro = '',
    [string]$StateRoot = (Join-Path $env:LOCALAPPDATA 'ArchieOS\SentryBridge'),
    [ValidateRange(1,300)][int]$WindowSeconds = 30,
    [ValidateRange(16,256)][int]$FileSizeMiB = 64,
    [ValidateRange(0,1000000)][int]$MaxWindows = 0,
    [switch]$PrepareOnly, [switch]$InstallStartup, [switch]$Status, [switch]$Stop
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Import-Module (Join-Path $PSScriptRoot 'HostCapture.psm1') -Force
$StateRoot = [IO.Path]::GetFullPath($StateRoot)
$configPath = Join-Path $StateRoot 'bridge.json'
$healthPath = Join-Path $StateRoot 'health.json'
$stopPath = Join-Path $StateRoot 'stop'
$wsl = "$env:WINDIR\System32\wsl.exe"
$taskName = 'ArchieOS-SentryHostBridge'

if ($Status) {
    if (Test-Path -LiteralPath $healthPath) { Get-Content -LiteralPath $healthPath -Raw }
    else { Write-Output '{"status":"not_configured"}' }
    return
}
if ($Stop) {
    if (-not (Test-Path -LiteralPath $StateRoot)) { throw 'Bridge is not configured' }
    [IO.File]::WriteAllText($stopPath, 'Requested by owner')
    Write-Output 'Stop requested; the worker stops its own capture and clears raw files.'
    return
}
Protect-SentryDirectory $StateRoot
$app = Join-Path $StateRoot 'app'
$rawRoot = Join-Path $StateRoot 'raw'
Protect-SentryDirectory $app
Protect-SentryDirectory $rawRoot

function Invoke-BridgeWsl {
    param([string[]]$Command, [int]$Timeout = 30)
    $r = Invoke-SentryNative -FilePath $wsl -Arguments (@('-d',$script:Distro,'--cd','/','--exec') + $Command) -TimeoutSeconds $Timeout
    if ($r.ExitCode -ne 0) { throw ('WSL helper failed: '+$r.Stderr+$r.Stdout) }
    return $r.Stdout.Trim()
}
function Wsl-Path([string]$Path) {
    Invoke-BridgeWsl -Command @('/usr/bin/wslpath','-a','-u',[IO.Path]::GetFullPath($Path))
}
function Write-Health([string]$State, [object]$Details) {
    Write-SentryJson $healthPath ([ordered]@{format='archie-sentry-host-health-v1';status=$State;
        updated_utc=[DateTime]::UtcNow.ToString('o');pid=$PID;details=$Details;
        gateway_wan_observed=$false;capture_scope='all-host-nics';raw_limit_mib=$FileSizeMiB})
}

if (Test-Path -LiteralPath $configPath) {
    $config = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
    if (-not $Distro) { $Distro = [string]$config.distro }
}
if (-not $Distro) {
    $list = Invoke-SentryNative -FilePath $wsl -Arguments @('-l','-q')
    if ($list.ExitCode -ne 0) { throw 'Cannot list WSL distributions' }
    $available = @($list.Stdout.Replace("`0",'').Split("`n") | ForEach-Object { $_.Trim() } | Where-Object { $_ -match '^ArchieOS' })
    if ($available.Count -ne 1) { throw 'Choose the ArchieOS distribution once with -Distro NAME' }
    $Distro = $available[0]
}

if ($PrepareOnly -or -not (Test-Path -LiteralPath $configPath)) {
    $repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
    if (-not (Test-Path -LiteralPath (Join-Path $repo 'core\Cargo.toml'))) { throw 'Prepare from the repository copy first' }
    Write-Output 'Preparing pinned Sentry binary; CPU build, no GPU/model loading.'
    $linuxHome = Invoke-BridgeWsl -Command @('/usr/bin/printenv','HOME')
    if (-not $linuxHome.StartsWith('/')) { throw 'Invalid Linux home directory' }
    $target = $linuxHome + '/.cache/sentry-bridge-build'
    $binary = $linuxHome + '/.local/lib/archie-sentry-bridge/archie-netwatch'
    $linuxState = $linuxHome + '/.local/state/archie-sentry-bridge'
    $manifest = (Wsl-Path (Join-Path $repo 'core\Cargo.toml'))
    Invoke-BridgeWsl -Command @('/usr/bin/env',('CARGO_TARGET_DIR='+$target),'CARGO_BUILD_JOBS=2','RUSTFLAGS=-Cdebuginfo=0',
        '/usr/bin/cargo','build','--locked','--manifest-path',$manifest,'-p','archie-lan','--bin','archie-netwatch') -Timeout 600 | Out-Null
    Invoke-BridgeWsl -Command @('/usr/bin/install','-D','-m','755',($target+'/debug/archie-netwatch'),$binary) | Out-Null
    $schema = Invoke-BridgeWsl -Command @($binary,'capture-fields') | ConvertFrom-Json
    if ($schema.format -ne 'archie-sentry-fields-v1' -or $schema.snaplen -ne 768) { throw 'Wrong Sentry binary/catalogue' }
    foreach ($file in @('Start-SentryHostBridge.ps1','HostCapture.psm1','analyze.py')) {
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot $file) -Destination (Join-Path $app $file) -Force
    }
    $installId = [Guid]::NewGuid().ToString('N')
    if (Test-Path -LiteralPath $configPath) {
        $prior = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
        if ($prior.PSObject.Properties.Name -contains 'install_id') { $installId = $prior.install_id }
    }
    $config = [ordered]@{format='archie-sentry-host-config-v1';install_id=$installId;distro=$Distro;binary=$binary;
        linux_state=$linuxState;analyzer=(Wsl-Path (Join-Path $app 'analyze.py'));
        window_seconds=$WindowSeconds;file_size_mib=$FileSizeMiB;owner_sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value}
    Write-SentryJson $configPath $config
    Write-Health 'prepared' @{note='Capture requires one Windows elevation; gateway authentication remains separate'}
    if ($PrepareOnly) { Write-Output $configPath; return }
    $config = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
}

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
if ($identity.User.Value -ne $config.owner_sid) { throw 'Run the bridge as its configured Windows owner' }
if (-not ([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Windows elevation is required for PktMon. Use the prepared launcher with Run as administrator.'
}

if ($InstallStartup) {
    $description = Get-SentryTaskDescription $config.install_id
    $existing = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
    Assert-SentryTaskOwnership -Task $existing -InstallId $config.install_id -OwnerSid $identity.User.Value -OwnerName $identity.Name
    $arguments = @('-NoProfile','-ExecutionPolicy','Bypass','-File',(Join-Path $app 'Start-SentryHostBridge.ps1'),
                   '-StateRoot',$StateRoot,'-Distro',$Distro,'-WindowSeconds',[string]$config.window_seconds,
                   '-FileSizeMiB',[string]$config.file_size_mib) | ForEach-Object { ConvertTo-SentryArgument $_ }
    $action = New-ScheduledTaskAction -Execute "$env:WINDIR\System32\WindowsPowerShell\v1.0\powershell.exe" -Argument ($arguments -join ' ')
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $identity.Name
    $principal = New-ScheduledTaskPrincipal -UserId $identity.Name -LogonType Interactive -RunLevel Highest
    $settings = New-ScheduledTaskSettingsSet -MultipleInstances IgnoreNew -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1) -ExecutionTimeLimit ([TimeSpan]::Zero)
    Register-ScheduledTask -TaskName $taskName -Description $description -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null
    if (Test-Path -LiteralPath $stopPath) { Remove-Item -LiteralPath $stopPath }
    Start-ScheduledTask -TaskName $taskName
    Write-Output ('Autonomous bridge registered: '+$taskName)
    return
}
if (Test-Path -LiteralPath $stopPath) { Write-Health 'stopped' @{reason='owner request'}; return }

$mutex = New-Object Threading.Mutex($false,'Global\ArchieOSSentryPktmonBridge')
$locked = $false
$intentPath = Join-Path $StateRoot 'capture-owner.json'
$sessionActive = $false
try {
    try { $locked = $mutex.WaitOne(0) } catch [Threading.AbandonedMutexException] { $locked = $true }
    if (-not $locked) { Write-Output 'Existing bridge owns capture; leaving it running.'; return }
    # Recover only a prior own orphan whose exact random ETL path is still the active log.
    Repair-SentryCaptureJournal -Root $rawRoot -IntentPath $intentPath
    $windows = 0
    while (-not (Test-Path -LiteralPath $stopPath)) {
        $driver = Get-CimInstance Win32_SystemDriver -Filter "Name='PktMon'"
        if ($null -eq $driver) { throw 'Windows PktMon driver is unavailable' }
        if ($driver.State -ne 'Stopped') {
            $filters = Invoke-SentryNative -FilePath "$env:WINDIR\System32\pktmon.exe" -Arguments @('filter','list')
            if ($filters.ExitCode -ne 0) { throw 'Cannot inspect existing capture filters' }
            Assert-SentryEmptyFilters $filters.Stdout
        }
        $session = [Guid]::NewGuid().ToString('N')
        $etl = Join-Path $rawRoot ($session+'.etl')
        $pcap = Join-Path $rawRoot ($session+'.pcapng')
        $inventoryPath = Join-Path $StateRoot 'host.json'
        $latest = Join-Path $StateRoot 'latest.json'
        $inventory = Get-SentryHostInventory
        $componentList = Invoke-SentryNative -FilePath "$env:WINDIR\System32\pktmon.exe" -Arguments @('list','--json')
        if ($componentList.ExitCode -ne 0) { throw 'Cannot enumerate PktMon components' }
        $components = @(Select-SentryCaptureComponents -Groups ($componentList.Stdout | ConvertFrom-Json) -Adapters $inventory.adapters)
        Write-SentryJson $inventoryPath $inventory
        $intent = @{pid=$PID;started_ticks=(Get-Process -Id $PID).StartTime.ToUniversalTime().Ticks;etl=$etl;phase='pending'}
        Write-SentryJson $intentPath $intent
        $began = [DateTime]::UtcNow.ToString('o')
        Write-Health 'capturing' @{session=$session;started=$began;gateway=$inventory.gateway;gateway_ssh='not authenticated by host bridge'}
        $ownership = @{Status='not_started'}
        try {
            Invoke-SentryCaptureWindow -EtlPath $etl -Seconds $WindowSeconds -FileSizeMiB $FileSizeMiB -StopFile $stopPath -Ownership $ownership
            $intent.phase = 'stopped'
            Write-SentryJson $intentPath $intent
            Write-Health 'analyzing' @{session=$session;capture_started=$began;capture_finished=[DateTime]::UtcNow.ToString('o')}
            $componentResult = Join-Path $StateRoot 'component-latest.json'
            $aggregate = [ordered]@{format='archie-sentry-host-window-v1';session=$session;status='analyzed';interfaces=@();components=@();
                gateway_wan_observed=$false;packet_payloads_retained=$false;
                cross_interface_totals='not summed: duplicate observations may exist';
                loopback_coverage='not guaranteed by NIC capture';component_count=$components.Count}
            foreach ($component in $components) {
                # PktMon's combined PCAPNG labels every NIC "unknown". Convert each
                # component separately and preserve the authoritative GUID mapping.
                $converted = Invoke-SentryNative -FilePath "$env:WINDIR\System32\pktmon.exe" -Arguments @('etl2pcap',$etl,'--out',$pcap,'--component-id',[string]$component.id) -TimeoutSeconds 60
                if ($converted.ExitCode -ne 0) { throw ('PktMon component conversion failed: '+$converted.Stdout+$converted.Stderr) }
                $inventory['pktmon_component_filter'] = $component
                Write-SentryJson $inventoryPath $inventory
                Invoke-BridgeWsl -Command @('/usr/bin/python3',[string]$config.analyzer,'--capture',(Wsl-Path $pcap),
                    '--config',(Wsl-Path $inventoryPath),'--netwatch',[string]$config.binary,'--state',[string]$config.linux_state,
                    '--out',(Wsl-Path $componentResult),'--session',$session) -Timeout 210 | Out-Null
                $part = Get-Content -LiteralPath $componentResult -Raw | ConvertFrom-Json
                $aggregate.interfaces += @($part.interfaces)
                $aggregate.components += @{identity=$component;status=$part.status;observation_points=@($part.interfaces).Count;capture_sha256=$part.capture_file_sha256}
                Remove-SentryRawFiles -Root $rawRoot -Paths @($pcap)
            }
            Write-SentryJson $latest $aggregate
            $windows++
            Write-Health 'healthy' @{session=$session;windows_completed=$windows;latest=$latest;
                capture_started=$began;analysis_finished=[DateTime]::UtcNow.ToString('o');
                capture_gaps='conversion/analysis between finite windows; not lossless continuous capture'}
        } finally {
            # Keep intent on uncertain stop failure; never stop a session we cannot identify.
            $sessionActive = $ownership.Status -in @('active','unknown')
            if (-not $sessionActive) {
                $intent.phase = 'stopped'
                Write-SentryJson $intentPath $intent
                Complete-SentryCaptureCleanup -Root $rawRoot -EtlPath $etl -IntentPath $intentPath
            }
        }
        if ($MaxWindows -gt 0 -and $windows -ge $MaxWindows) { break }
    }
    Write-Health 'stopped' @{windows_completed=$windows;reason='requested or finite verification complete';latest=(Join-Path $StateRoot 'latest.json')}
} catch {
    Write-Health 'failed' @{error=$_.Exception.Message;capture_may_need_owner_recovery=$sessionActive}
    throw
} finally {
    if ($locked) { $mutex.ReleaseMutex() }
    $mutex.Dispose()
}
