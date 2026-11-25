param(
  [Parameter(Mandatory, Position=0)][string]$Item,
  [Parameter(Mandatory, Position=1)][ValidateSet('doing','done','todo')][string]$Status,
  [Parameter(Position=2)][string]$Pr,
  [Parameter(Position=3)][string]$Note
)
$ErrorActionPreference = 'Stop'
$config = @{}
Get-Content -Encoding UTF8 -LiteralPath (Join-Path $HOME '.config/bae-board/env') | ForEach-Object {
  if ($_ -match '^([^=]+)=(.*)$') { $config[$Matches[1]] = $Matches[2] }
}
$agent = $env:BAE_BOARD_AGENT
if (!$agent) { $agent = $config['BAE_BOARD_AGENT'] }
if (!$agent) {
  $branch = git branch --show-current
  $agent = "$env:COMPUTERNAME/$branch"
}
$data = @{agent=$agent; item=$Item; status=$Status}
if ($Pr) { $data.pr = $Pr }
if ($Note) { $data.note = $Note }
$body = [Text.Encoding]::UTF8.GetBytes(($data | ConvertTo-Json -Compress))
Invoke-RestMethod -Method Post -Uri ($config.BAE_BOARD_URL.TrimEnd('/') + '/api/report') -Headers @{Authorization="Bearer $($config.BAE_BOARD_TOKEN)"} -ContentType 'application/json; charset=utf-8' -Body $body -TimeoutSec 15
