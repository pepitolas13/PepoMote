param([string]$Receiver = "$PSScriptRoot/../target/debug/PepoMote.exe")
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$validation = Join-Path $repo 'dist/update-validation'
$root = Join-Path $validation ('helper-e2e-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root -Force | Out-Null
$source = @'
use std::{env,fs,path::Path,time::Duration};
fn main() {
    const VERSION:&str = "FIXTURE_VERSION";
    if VERSION=="1.11.0" && env::var_os("PEPOMOTE_FIXTURE_FAIL").is_some() { std::process::exit(7); }
    if let Some(path)=env::var_os("PEPOMOTE_UPDATE_HEALTH") {
        fs::write(path,format!("{}:{}",VERSION,std::process::id())).unwrap();
    } else if let Some(path)=env::var_os("PEPOMOTE_UPDATE_RESULT") {
        fs::write(Path::new(&path).parent().unwrap().join("restarted-old"),VERSION).unwrap();
    }
    std::thread::sleep(Duration::from_secs(1));
}
'@
foreach ($version in @('1.10.0','1.11.0')) {
    $src = Join-Path $root "fixture-$version.rs"
    [IO.File]::WriteAllText($src, $source.Replace('FIXTURE_VERSION',$version))
    & rustc --edition=2021 --crate-name fixture $src -o (Join-Path $root "fixture-$version.exe")
    if ($LASTEXITCODE -ne 0) { throw "Fixture compilation failed" }
}
function Wait-File([string]$Path,[int]$Seconds=15) {
    $until=[DateTime]::UtcNow.AddSeconds($Seconds)
    while (-not (Test-Path -LiteralPath $Path)) {
        if ([DateTime]::UtcNow -gt $until) { throw "Timeout waiting for $Path" }
        Start-Sleep -Milliseconds 50
    }
}
foreach ($failed in @($false,$true)) {
    $case=Join-Path $root $(if($failed){'rollback'}else{'success'})
    $work=Join-Path $case '.pepomote-update-fixture'
    New-Item -ItemType Directory -Path $work -Force | Out-Null
    $destination=Join-Path $case 'PepoMote.exe'
    $staged=Join-Path $work 'package'
    $helper=Join-Path $work 'helper.exe'
    Copy-Item -LiteralPath (Join-Path $root 'fixture-1.10.0.exe') -Destination $destination
    Copy-Item -LiteralPath (Join-Path $root 'fixture-1.11.0.exe') -Destination $staged
    Copy-Item -LiteralPath $Receiver -Destination $helper
    $oldHash=(Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
    $newHash=(Get-FileHash -LiteralPath $staged -Algorithm SHA256).Hash.ToLowerInvariant()
    $parent=Start-Process -FilePath powershell.exe -ArgumentList @('-NoProfile','-NonInteractive','-Command','[Threading.Thread]::Sleep(30000)') -WindowStyle Hidden -PassThru
    try {
        $plan=@{work=$work;destination=$destination;staged=$staged;bundle=$false;version='1.11.0';parent_pid=$parent.Id;launch_args=@();original_sha256=$oldHash;
            asset=@{name='PepoMote.exe';url='https://github.com/pepitolas13/PepoMote/releases/download/v1.11.0/PepoMote.exe';size=(Get-Item -LiteralPath $staged).Length;sha256=$newHash}}
        $planPath=Join-Path $work 'plan.json'
        [IO.File]::WriteAllText($planPath,($plan|ConvertTo-Json -Depth 6),[Text.UTF8Encoding]::new($false))
        if($failed){$env:PEPOMOTE_FIXTURE_FAIL='1'}else{Remove-Item Env:PEPOMOTE_FIXTURE_FAIL -ErrorAction SilentlyContinue}
        $helperProcess=Start-Process -FilePath $helper -ArgumentList @('--pepomote-apply-update',('"'+$planPath+'"')) -WindowStyle Hidden -PassThru
        Remove-Item Env:PEPOMOTE_FIXTURE_FAIL -ErrorAction SilentlyContinue
        Wait-File (Join-Path $work 'ready')
        $nonce=[guid]::NewGuid().ToString('N')
        [IO.File]::WriteAllText((Join-Path $work 'commit.tmp'),$nonce)
        Move-Item -LiteralPath (Join-Path $work 'commit.tmp') -Destination (Join-Path $work 'commit')
        Wait-File (Join-Path $work 'committed')
        $ack=[IO.File]::ReadAllText((Join-Path $work 'committed'))
        if($ack -ne "$($helperProcess.Id):$nonce"){throw 'Commit acknowledgment mismatch'}
        if((Get-FileHash -LiteralPath $destination).Hash.ToLowerInvariant() -ne $oldHash){throw 'Installer changed files while parent was running'}
        Stop-Process -Id $parent.Id
        Wait-File (Join-Path $work 'result') 20
        $result=[IO.File]::ReadAllText((Join-Path $work 'result'))
        $expected=if($failed){'rolled-back'}else{'installed'}
        if($result -ne $expected){throw "Expected $expected, got $result"}
        $expectedHash=if($failed){$oldHash}else{$newHash}
        if((Get-FileHash -LiteralPath $destination).Hash.ToLowerInvariant() -ne $expectedHash){throw 'Installed file hash mismatch'}
        if($failed){Wait-File (Join-Path $work 'restarted-old');if([IO.File]::ReadAllText((Join-Path $work 'restarted-old')) -ne '1.10.0'){throw 'Old fixture was not relaunched'}}
        $helperProcess.WaitForExit(5000)|Out-Null
        Write-Output "PASS $expected : native helper acknowledgment, parent-exit gating, verified replacement and process outcome"
    } finally {
        if(-not $parent.HasExited){Stop-Process -Id $parent.Id -ErrorAction SilentlyContinue}
        Remove-Item Env:PEPOMOTE_FIXTURE_FAIL -ErrorAction SilentlyContinue
    }
}
Write-Output "Isolated artifacts: $root"
