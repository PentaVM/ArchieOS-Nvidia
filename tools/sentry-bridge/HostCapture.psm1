Set-StrictMode -Version Latest

function ConvertTo-SentryArgument {
    param([AllowEmptyString()][string]$Value)
    if ($Value.Length -gt 0 -and $Value -notmatch '[\s"]') { return $Value }
    $escaped = [regex]::Replace($Value, '(\\*)"', '$1$1\"')
    $escaped = [regex]::Replace($escaped, '(\\+)$', '$1$1')
    return '"' + $escaped + '"'
}

function Invoke-SentryNative {
    param([string]$FilePath, [string[]]$Arguments, [int]$TimeoutSeconds = 30)
    $info = New-Object System.Diagnostics.ProcessStartInfo
    $info.FileName = $FilePath
    $info.Arguments = (($Arguments | ForEach-Object { ConvertTo-SentryArgument $_ }) -join ' ')
    $info.UseShellExecute = $false; $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true; $info.RedirectStandardError = $true
    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $info
    try {
        if (-not $process.Start()) { throw 'Could not start the owned helper' }
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
            $process.Kill(); $process.WaitForExit()
            throw ('Owned helper timed out: ' + [IO.Path]::GetFileName($FilePath))
        }
        [pscustomobject]@{ExitCode=$process.ExitCode;Stdout=$stdout.Result;Stderr=$stderr.Result}
    } finally { $process.Dispose() }
}

function Get-SentryCaptureArguments {
    param([string]$EtlPath, [ValidateRange(16,256)][int]$FileSizeMiB = 64)
    @('start','--capture','--comp','nics','--pkt-size','768',
      '--file-name',$EtlPath,'--file-size',[string]$FileSizeMiB,'--log-mode','circular')
}

function Assert-SentryEmptyFilters {
    param([string]$Text)
    if ($Text.Trim() -notmatch '^(No (packet )?filters([ .]|$)|No hay filtros([ .]|$)|No se han configurado filtros([ .]|$)|No existen filtros([ .]|$)|Filtros de paquete:\s+Ninguno\s*$)') {
        throw 'PktMon filters are nonempty or unknown; refusing to change another capture configuration.'
    }
}

function Select-SentryGateway {
    param([object[]]$Routes, [int[]]$PhysicalIndices)
    $route = $Routes | Where-Object { $_.InterfaceIndex -in $PhysicalIndices -and $_.NextHop -ne '0.0.0.0' } |
        Sort-Object RouteMetric | Select-Object -First 1
    if ($null -ne $route) { return [string]$route.NextHop }
    return $null
}

function Select-SentryCaptureComponents {
    param([object[]]$Groups, [object[]]$Adapters)
    $found = @{}
    foreach ($group in $Groups) {
        foreach ($component in $group.Components) {
            $guid = @($component.Properties | Where-Object Name -eq 'ifGuid')
            if ($guid.Count -ne 1) { continue }
            foreach ($adapter in $Adapters) {
                if (([string]$guid[0].Value).Trim('{}') -eq ([string]$adapter.InterfaceGuid).Trim('{}')) {
                    $id = [int]$component.Id
                    if ($id -lt 1 -or $id -gt 65535) { throw 'Invalid PktMon component id' }
                    $found[$id] = @{id=$id;adapter_guid=([string]$adapter.InterfaceGuid).Trim('{}');adapter_name=[string]$adapter.Name}
                }
            }
        }
    }
    if ($found.Count -lt 1 -or $found.Count -gt 64) { throw 'Cannot establish bounded host NIC component provenance' }
    return @($found.Values | Sort-Object id)
}

function Get-SentryHostInventory {
    $adapters = @(Get-NetAdapter -IncludeHidden | Where-Object Status -eq 'Up')
    $physical = @(Get-NetAdapter -Physical | Where-Object Status -eq 'Up')
    $routes = @(Get-NetRoute -AddressFamily IPv4 -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue)
    $addresses = @(Get-NetIPAddress -ErrorAction Stop | Where-Object {
        $_.InterfaceIndex -in $adapters.ifIndex -and $_.IPAddress -notin @('127.0.0.1','::1')
    } | Select-Object -ExpandProperty IPAddress -Unique)
    $onLink = @(Get-NetRoute -ErrorAction Stop | Where-Object {
        $_.InterfaceIndex -in $adapters.ifIndex -and $_.NextHop -in @('0.0.0.0','::')
    } | Select-Object -ExpandProperty DestinationPrefix -Unique)
    [ordered]@{local_addresses=$addresses;gateway=(Select-SentryGateway $routes @($physical.ifIndex));
        on_link_prefixes=$onLink;
        adapters=@($adapters | Select-Object Name,InterfaceDescription,ifIndex,MacAddress,InterfaceGuid);
        capture_scope='all-host-nics';gateway_wan_observed=$false;loopback_coverage='not guaranteed'}
}

function Write-SentryJson {
    param([string]$Path, [object]$Value)
    $partial = $Path + '.' + [Guid]::NewGuid().ToString('N') + '.partial'
    [IO.File]::WriteAllText($partial, ($Value | ConvertTo-Json -Depth 35), (New-Object Text.UTF8Encoding($false)))
    Move-Item -LiteralPath $partial -Destination $Path -Force
}

function Protect-SentryDirectory {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) { New-Item -ItemType Directory -Path $Path -Force | Out-Null }
    if ((Get-Item -LiteralPath $Path).Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw 'Capture directory cannot be a junction or symbolic link'
    }
    # Preserve the security descriptor sections we did not read (notably SACL).
    # Windows PowerShell Set-Acl on a blank descriptor requests SeSecurityPrivilege.
    $acl = Get-Acl -LiteralPath $Path
    $acl.SetAccessRuleProtection($true,$false)
    foreach ($rule in @($acl.Access)) { $acl.RemoveAccessRuleSpecific($rule) }
    $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
    foreach ($id in @($sid.Value,'S-1-5-18','S-1-5-32-544') | Select-Object -Unique) {
        $identity = New-Object Security.Principal.SecurityIdentifier($id)
        $rule = New-Object Security.AccessControl.FileSystemAccessRule($identity,'FullControl','ContainerInherit,ObjectInherit','None','Allow')
        $acl.AddAccessRule($rule)
    }
    $acl.SetOwner($sid)
    if ($PSVersionTable.PSVersion.Major -le 5) { [IO.Directory]::SetAccessControl($Path,$acl) }
    else { Set-Acl -LiteralPath $Path -AclObject $acl }
}

function Remove-SentryRawFiles {
    param([string]$Root, [string[]]$Paths)
    $absolute = [IO.Path]::GetFullPath($Root).TrimEnd('\')
    if ((Get-Item -LiteralPath $absolute).Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw 'Raw capture root is a reparse point'
    }
    foreach ($path in $Paths) {
        $file = [IO.Path]::GetFullPath($path)
        if ([IO.Path]::GetDirectoryName($file) -ne $absolute -or [IO.Path]::GetExtension($file) -notin @('.etl','.pcapng')) {
            throw 'Refusing cleanup outside the exact owned raw capture directory'
        }
        if (Test-Path -LiteralPath $file) { Remove-Item -LiteralPath $file -Force }
    }
}

function Test-SentryCaptureStatusOwned {
    param([string]$Text, [string]$EtlPath)
    $expected = [IO.Path]::GetFullPath($EtlPath)
    foreach ($line in ($Text -split '\r?\n')) {
        $value = $line.Trim()
        if ($value -match '^[^:]+:\s*(.*)$') { $value = $Matches[1].Trim() }
        if ($value.StartsWith('"') -and $value.EndsWith('"')) { $value = $value.Substring(1,$value.Length-2) }
        if ([String]::Equals($value,$expected,[StringComparison]::OrdinalIgnoreCase)) { return $true }
    }
    return $false
}

function Stop-SentryOwnedCapture {
    param([string]$EtlPath)
    $current = Invoke-SentryNative -FilePath "$env:WINDIR\System32\pktmon.exe" -Arguments @('status')
    if ($current.ExitCode -ne 0 -or -not (Test-SentryCaptureStatusOwned $current.Stdout $EtlPath)) {
        throw 'Cannot prove exact ownership of the active PktMon session; refusing to stop it'
    }
    $stop = Invoke-SentryNative -FilePath "$env:WINDIR\System32\pktmon.exe" -Arguments @('stop')
    if ($stop.ExitCode -ne 0) { throw ('Owned PktMon stop failed; inspect before another capture: '+$stop.Stdout+$stop.Stderr) }
}

function Complete-SentryCaptureCleanup {
    param([string]$Root, [string]$EtlPath, [string]$IntentPath)
    Remove-SentryRawFiles -Root $Root -Paths @($EtlPath,[IO.Path]::ChangeExtension($EtlPath,'.pcapng'))
    if (Test-Path -LiteralPath $IntentPath) { Remove-Item -LiteralPath $IntentPath -Force }
}

function Repair-SentryCaptureJournal {
    param([string]$Root, [string]$IntentPath)
    if (-not (Test-Path -LiteralPath $IntentPath)) { return }
    $previous = Get-Content -LiteralPath $IntentPath -Raw | ConvertFrom-Json
    $oldProcess = Get-Process -Id $previous.pid -ErrorAction SilentlyContinue
    if ($oldProcess -and $oldProcess.StartTime.ToUniversalTime().Ticks -eq $previous.started_ticks) {
        throw 'Existing bridge owner is still alive'
    }
    $priorFile = [IO.Path]::GetFullPath([string]$previous.etl)
    if ([IO.Path]::GetDirectoryName($priorFile) -ne [IO.Path]::GetFullPath($Root).TrimEnd('\') -or
        [IO.Path]::GetFileName($priorFile) -notmatch '^[a-fA-F0-9]{32}\.etl$') {
        throw 'Invalid prior capture ownership record'
    }
    # Older journals have no phase and must meet the same active-session proof.
    if (-not ($previous.PSObject.Properties.Name -contains 'phase') -or $previous.phase -ne 'stopped') {
        Stop-SentryOwnedCapture -EtlPath $priorFile
        $previous | Add-Member -NotePropertyName phase -NotePropertyValue 'stopped' -Force
        Write-SentryJson $IntentPath $previous
    }
    Complete-SentryCaptureCleanup -Root $Root -EtlPath $priorFile -IntentPath $IntentPath
}

function Get-SentryTaskDescription {
    param([string]$InstallId)
    if ($InstallId -notmatch '^[a-fA-F0-9]{32}$') { throw 'Invalid bridge installation identity' }
    return 'ArchieOS Sentry Host Bridge installation ' + $InstallId
}

function Assert-SentryTaskOwnership {
    param([object]$Task, [string]$InstallId, [string]$OwnerSid, [string]$OwnerName)
    if ($null -eq $Task) { return }
    $sameOwner = $Task.Principal.UserId -in @($OwnerSid,$OwnerName)
    if (-not $sameOwner) {
        try {
            $principalName = [string]$Task.Principal.UserId
            if ($principalName -notmatch '[\\@]') { $principalName = [Environment]::MachineName + '\' + $principalName }
            $account = New-Object Security.Principal.NTAccount($principalName)
            $sameOwner = $account.Translate([Security.Principal.SecurityIdentifier]).Value -eq $OwnerSid
        } catch { $sameOwner = $false }
    }
    if ($Task.Description -ne (Get-SentryTaskDescription $InstallId) -or -not $sameOwner) {
        throw 'Scheduled task belongs to another installation or account; refusing to overwrite it'
    }
}

function Invoke-SentryCaptureWindow {
    param([string]$EtlPath, [ValidateRange(1,300)][int]$Seconds = 30,
          [ValidateRange(16,256)][int]$FileSizeMiB = 64, [string]$StopFile = '',
          [hashtable]$Ownership = @{})
    $owned = $false
    try {
        $Ownership.Status = 'unknown'
        $start = Invoke-SentryNative -FilePath "$env:WINDIR\System32\pktmon.exe" -Arguments @(Get-SentryCaptureArguments $EtlPath $FileSizeMiB)
        if ($start.ExitCode -ne 0) {
            $Ownership.Status = 'unowned'
            throw ('PktMon did not start an owned session: '+$start.Stdout+$start.Stderr)
        }
        $owned = $true
        $Ownership.Status = 'active'
        for ($i=0; $i -lt $Seconds; $i++) {
            if ($StopFile -and (Test-Path -LiteralPath $StopFile)) { break }
            Start-Sleep -Seconds 1
        }
    } finally {
        if ($owned) {
            Stop-SentryOwnedCapture -EtlPath $EtlPath
            $Ownership.Status = 'stopped'
        }
    }
}

Export-ModuleMember -Function *-Sentry*
