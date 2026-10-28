param(
    [string]$Distribution = 'ArchieOS-Operator-RTX3070-Test',
    [string]$LinuxUser = 'archie',
    [string]$NativeBinary = '/home/archie/.local/share/archie-world-demo/archie-display',
    [ValidateSet('google_satellite', 'sentinel2', 'blue_marble', 'landsat', 'natural_color', 'infrared', 'night_lights', 'thermal')]
    [string]$ImagerySource = 'google_satellite'
)
$ErrorActionPreference = 'Stop'
$repository = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$linuxPathOutput = & wsl.exe -d $Distribution -u $LinuxUser --exec wslpath -u $repository.Replace('\', '/')
if ($LASTEXITCODE -ne 0) {
    throw 'No se pudo resolver el repositorio dentro de la distro indicada.'
}
$linuxRepository = ($linuxPathOutput -join "`n").Trim()
if (-not $linuxRepository.StartsWith('/')) {
    throw 'La distro no devolvió una ruta absoluta para el repositorio.'
}
$binaryDirectory = "$linuxRepository/core/target/linux-validation/debug"
$helper = "$linuxRepository/_dev/verification/world-globe/globe_demo.py"
$runDirectory = '/home/archie/.local/share/archie-world-demo/run-' + [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
Write-Host 'Abriendo World desde la Tierra completa. Arrastra para navegar y usa la rueda para cargar más detalle. Google se mantiene en RAM; su fecha no está publicada.'
Write-Host 'Mientras la ventana siga abierta, su sesión se renueva cada hora con el mismo presupuesto y fecha. Una parada explícita se respeta.'
Write-Host 'Esta demostración usa los puertos locales 17666/17667/17668. Cierra la ventana para detener sus procesos.'
& wsl.exe -d $Distribution -u $LinuxUser --exec python3 $helper --bin-dir $binaryDirectory `
    --run-dir $runDirectory --native-bin $NativeBinary --source $ImagerySource --renew-navigation
if ($LASTEXITCODE -ne 0) {
    throw "La demostración terminó con error. Registros conservados en $runDirectory"
}
