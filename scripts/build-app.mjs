import { copyFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';

function run(command, args) {
  const result = spawnSync(command, args, { stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} exited with code ${result.status}`);
}

function requireDotnetSdk() {
  const result = spawnSync('dotnet', ['--list-sdks'], { encoding: 'utf8' });
  if (result.error || result.status !== 0 || !/^8\.\d+\.\d+\s+\[/m.test(result.stdout ?? '')) {
    throw new Error(
      'Build helper sensor memerlukan .NET 8 SDK. Instal dari https://dotnet.microsoft.com/download/dotnet/8.0 lalu jalankan ulang build.'
    );
  }
}

if (process.platform === 'win32') requireDotnetSdk();
run('node', ['scripts/build-stations.mjs']);
if (process.platform === 'win32') {
  const artifactsPath = mkdtempSync(join(tmpdir(), 'dynamic-island-sensor-'));
  try {
    run('dotnet', [
      'publish',
      'sensor/HardwareSensor.csproj',
      '--configuration', 'Release',
      '--runtime', 'win-x64',
      '--self-contained', 'true',
      '--artifacts-path', artifactsPath,
      '-p:PublishSingleFile=true',
      '-p:IncludeNativeLibrariesForSelfExtract=true',
      '-p:DebugType=None',
      '-p:DebugSymbols=false',
      '--output', 'src-tauri/resources',
    ]);
    copyFileSync('sensor/LICENSE-LibreHardwareMonitor.txt', 'src-tauri/resources/LibreHardwareMonitor-LICENSE.txt');
  } finally {
    rmSync(artifactsPath, { recursive: true, force: true });
  }
}
