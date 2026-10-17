Import-Module (Join-Path $PSScriptRoot '../HostCapture.psm1') -Force

Describe 'Bounded Windows capture plan' {
    It 'passes native arguments literally without a shell or lost trailing slash' {
        $expected = @('path with spaces\','quote"value','$(not execution); & nope','')
        $python = (Get-Command python.exe -ErrorAction Stop).Source
        $r = Invoke-SentryNative -FilePath $python -Arguments (@('-c','import sys,json; print(json.dumps(sys.argv[1:]))') + $expected)
        $r.ExitCode | Should Be 0
        $actual = $r.Stdout | ConvertFrom-Json
        $actual.Count | Should Be $expected.Count
        for ($i=0;$i -lt $expected.Count;$i++) { $actual[$i] | Should Be $expected[$i] }
    }
    It 'can protect a private directory without requesting SACL privileges' {
        $private = Join-Path $TestDrive 'acl'
        Protect-SentryDirectory $private
        (Get-Acl -LiteralPath $private).AreAccessRulesProtected | Should Be $true
    }
    It 'captures every NIC with bounded circular storage and the Sentry snaplen' {
        $a = @(Get-SentryCaptureArguments -EtlPath 'C:\private\capture.etl' -FileSizeMiB 64)
        ($a -join ' ') | Should Match '--comp nics'
        ($a -join ' ') | Should Match '--pkt-size 768'
        ($a -join ' ') | Should Match '--file-size 64'
        ($a -join ' ') | Should Match '--log-mode circular'
        ($a -contains 'C:\private\capture.etl') | Should Be $true
    }
    It 'chooses the physical gateway instead of WSL NAT' {
        $routes = @([pscustomobject]@{InterfaceIndex=31;NextHop='172.31.32.1';RouteMetric=1},
                    [pscustomobject]@{InterfaceIndex=7;NextHop='192.168.1.1';RouteMetric=10})
        Select-SentryGateway -Routes $routes -PhysicalIndices @(7) | Should Be '192.168.1.1'
    }
    It 'rejects unrecognised or nonempty global filters rather than clearing them' {
        { Assert-SentryEmptyFilters -Text 'Filtro 1: DNS' } | Should Throw
        { Assert-SentryEmptyFilters -Text 'unknown status' } | Should Throw
        { Assert-SentryEmptyFilters -Text 'No filters.' } | Should Not Throw
        { Assert-SentryEmptyFilters -Text 'No hay filtros.' } | Should Not Throw
        { Assert-SentryEmptyFilters -Text "Filtros de paquete:`n    Ninguno" } | Should Not Throw
    }
    It 'maps component GUIDs rather than translated names or virtual route priority' {
        $groups = @([pscustomobject]@{Components=@(
            [pscustomobject]@{Id=11;Name='controller';Properties=@([pscustomobject]@{Name='ifGuid';Value='{aaaa}'})},
            [pscustomobject]@{Id=12;Name='filter';Properties=@([pscustomobject]@{Name='ifGuid';Value='{bbbb}'})})})
        $a = @([pscustomobject]@{Name='Ethernet';InterfaceGuid='aaaa'})
        $result = @(Select-SentryCaptureComponents $groups $a)
        $result.Count | Should Be 1
        $result[0].id | Should Be 11
        $result[0].adapter_name | Should Be 'Ethernet'
    }
    It 'does not let cleanup remove files outside its own raw folder' {
        $raw = Join-Path $TestDrive 'raw'; New-Item -ItemType Directory -Path $raw | Out-Null
        $outside = Join-Path $TestDrive 'keep.etl'; Set-Content $outside 'keep'
        { Remove-SentryRawFiles -Root $raw -Paths @($outside) } | Should Throw
        Test-Path $outside | Should Be $true
    }
}

InModuleScope HostCapture {
    Describe 'Ownership of the global PktMon session' {
        BeforeEach {
            Mock Start-Sleep {}
        }
        It 'never stops a session whose start failed' {
            Mock Invoke-SentryNative { [pscustomobject]@{ExitCode=1;Stdout='';Stderr='busy'} }
            { Invoke-SentryCaptureWindow -EtlPath 'C:\own.etl' -Seconds 1 } | Should Throw
            Assert-MockCalled Invoke-SentryNative -Times 0 -Exactly -Scope It -ParameterFilter { $Arguments[0] -eq 'stop' }
        }
        It 'stops its own session even when the capture wait throws' {
            Mock Invoke-SentryNative { [pscustomobject]@{ExitCode=0;Stdout='Log file: C:\own.etl';Stderr=''} }
            Mock Start-Sleep { throw 'interrupted' }
            { Invoke-SentryCaptureWindow -EtlPath 'C:\own.etl' -Seconds 1 } | Should Throw
            Assert-MockCalled Invoke-SentryNative -Times 1 -Exactly -Scope It -ParameterFilter { $Arguments[0] -eq 'stop' }
        }
        It 'treats a failed stop as failure, never healthy completion' {
            Mock Invoke-SentryNative {
                if ($Arguments[0] -eq 'stop') { [pscustomobject]@{ExitCode=1;Stdout='';Stderr='failed'} }
                else { [pscustomobject]@{ExitCode=0;Stdout='Log file: C:\own.etl';Stderr=''} }
            }
            { Invoke-SentryCaptureWindow -EtlPath 'C:\own.etl' -Seconds 1 } | Should Throw
        }
        It 'never stops a replacement session started by another operator' {
            Mock Invoke-SentryNative { [pscustomobject]@{ExitCode=0;Stdout='Log file: C:\other.etl';Stderr=''} }
            { Invoke-SentryCaptureWindow -EtlPath 'C:\own.etl' -Seconds 1 } | Should Throw
            Assert-MockCalled Invoke-SentryNative -Times 0 -Exactly -Scope It -ParameterFilter { $Arguments[0] -eq 'stop' }
        }
        It 'requires an exact ownership path rather than a substring' {
            Test-SentryCaptureStatusOwned 'Log file: C:\own.etl.extra' 'C:\own.etl' | Should Be $false
            Test-SentryCaptureStatusOwned 'Archivo: C:\OWN.etl' 'C:\own.etl' | Should Be $true
        }
        It 'recovers stopped raw files without issuing a global stop' {
            $raw = Join-Path $TestDrive 'recover'; New-Item -ItemType Directory $raw | Out-Null
            $etl = Join-Path $raw ('a'*32+'.etl'); Set-Content $etl 'raw'
            $journal = Join-Path $TestDrive 'journal.json'
            Write-SentryJson $journal @{pid=2147483647;started_ticks=0;etl=$etl;phase='stopped'}
            Mock Invoke-SentryNative { throw 'Must not touch a capture' }
            Repair-SentryCaptureJournal $raw $journal
            Test-Path $etl | Should Be $false
            Test-Path $journal | Should Be $false
        }
        It 'keeps the cleanup journal if raw deletion fails' {
            $journal = Join-Path $TestDrive 'retain.json'; Set-Content $journal 'keep'
            Mock Remove-SentryRawFiles { throw 'locked file' }
            { Complete-SentryCaptureCleanup $TestDrive 'C:\own.etl' $journal } | Should Throw
            Test-Path $journal | Should Be $true
        }
        It 'does not overwrite an unrelated scheduled task with the same name' {
            $task = [pscustomobject]@{Description='other application';Principal=[pscustomobject]@{UserId='user'}}
            { Assert-SentryTaskOwnership $task ('a'*32) 'sid' 'user' } | Should Throw
        }
        It 'recognizes the same Windows SID when Task Scheduler shortens the account name' {
            $owner = [Security.Principal.WindowsIdentity]::GetCurrent()
            $task = [pscustomobject]@{Description=(Get-SentryTaskDescription ('a'*32));Principal=[pscustomobject]@{UserId=$owner.Name.Split('\')[-1]}}
            { Assert-SentryTaskOwnership $task ('a'*32) $owner.User.Value $owner.Name } | Should Not Throw
        }
    }
}

