param(
    [Parameter(Mandatory=$true)][string]$WorldBinary,
    [Parameter(Mandatory=$true)][string]$CliBinary,
    [Parameter(Mandatory=$true)][string]$RunDirectory
)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $RunDirectory){throw 'Use a new smoke directory; captures are preserved.'}
New-Item -ItemType Directory -Path $RunDirectory | Out-Null
$geoRuntime=Join-Path $RunDirectory 'runtime'
$geoCache=Join-Path $RunDirectory 'cache'
$geoListener=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0)
$geoListener.Start()
$geoPort=$geoListener.LocalEndpoint.Port
$geoListener.Stop()
$geoOldRuntime=$env:ARCHIE_WORLD_RUNTIME_DIR
$geoProcess=$null
try {
    $env:ARCHIE_WORLD_RUNTIME_DIR=$geoRuntime
    $geoProcess=Start-Process -FilePath $WorldBinary -ArgumentList @('--bind',"127.0.0.1:$geoPort",'--enable-exploration','--domain','personal','--exploration-runtime',$geoRuntime,'--exploration-cache',$geoCache) -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $RunDirectory 'worldd.stdout.log') -RedirectStandardError (Join-Path $RunDirectory 'worldd.stderr.log')
    $geoUntil=[DateTime]::UtcNow.AddSeconds(20)
    while(-not(Test-Path (Join-Path $geoRuntime 'control.json'))){
        if($geoProcess.HasExited){throw 'Smoke world daemon exited; inspect its preserved log.'}
        if([DateTime]::UtcNow -gt $geoUntil){throw 'Private command endpoint did not start.'}
        Start-Sleep -Milliseconds 100
    }
    $geoAnonymous=Invoke-WebRequest "http://127.0.0.1:$geoPort/v1/exploration" -SkipHttpErrorCheck
    if($geoAnonymous.StatusCode -ne 403){throw 'Anonymous capture projection was not denied.'}
    $geoCredential=Get-Content -Raw (Join-Path $geoRuntime 'projection.json') | ConvertFrom-Json
    $geoAuthorized=Invoke-WebRequest "http://127.0.0.1:$geoPort/v1/exploration" -Headers @{Authorization="Bearer $($geoCredential.token)"}
    if($geoAuthorized.StatusCode -ne 200){throw 'Read-only projection credential did not work.'}
    $geoArgs=@('--latitude','41.8902','--longitude','12.4922','--radius-m','50','--zoom','14','--layers','terrain','--domain','personal','--max-tiles','16','--max-bytes','8388608','--json')
    $geoPlan=& $CliBinary world plan @geoArgs
    if($LASTEXITCODE -ne 0){throw 'CLI plan failed.'}
    $geoPlan | Set-Content (Join-Path $RunDirectory 'plan.json')
    $geoAccepted=& $CliBinary world explore @geoArgs
    if($LASTEXITCODE -ne 0){throw 'CLI exploration submission failed.'}
    $geoAccepted=$geoAccepted | ConvertFrom-Json
    $geoUntil=[DateTime]::UtcNow.AddSeconds(75)
    do {
        Start-Sleep -Milliseconds 500
        $geoStatusRaw=& $CliBinary world status --domain personal --json
        if($LASTEXITCODE -ne 0){throw 'CLI status failed.'}
        $geoStatus=$geoStatusRaw | ConvertFrom-Json
        $geoJob=$geoStatus.jobs | Where-Object id -EQ $geoAccepted.job.id
        if([DateTime]::UtcNow -gt $geoUntil){throw 'Live capture timed out; its files are preserved.'}
    } while($geoJob.state -notin @('complete','failed','cancelled'))
    $geoStatusRaw | Set-Content (Join-Path $RunDirectory 'status.json')
    if($geoJob.state -ne 'complete'){throw "Live source capture failed: $($geoJob.error)"}
    $geoPlaces=& $CliBinary world resolve 'Colosseum, Rome' --domain personal --json
    $geoGeocodeExit=$LASTEXITCODE
    if($geoGeocodeExit -eq 0){$geoPlaces | Set-Content (Join-Path $RunDirectory 'places.json')}
    [pscustomobject]@{
        anonymous_projection_status=$geoAnonymous.StatusCode
        authorized_projection_status=$geoAuthorized.StatusCode
        source='AWS Terrain Tiles · public Rome AOI'
        job_state=$geoJob.state
        downloaded_payload_bytes=$geoJob.bytes_received
        stored_bytes=$geoJob.stored_bytes
        tiles_done=$geoJob.tiles_done
        tiles_total=$geoJob.tiles_total
        artifacts=@($geoJob.artifacts | ForEach-Object kind)
        geocoder_exit=$geoGeocodeExit
        hardware_or_gpu_verified=$false
    } | ConvertTo-Json | Tee-Object -FilePath (Join-Path $RunDirectory 'summary.json')
} finally {
    $env:ARCHIE_WORLD_RUNTIME_DIR=$geoOldRuntime
    if($geoProcess -and -not $geoProcess.HasExited){Stop-Process -Id $geoProcess.Id}
}
