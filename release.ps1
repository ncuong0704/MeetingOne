param(
    [Parameter(Mandatory=$true)]
    [string]$Version
)

$configPath = "frontend/src-tauri/tauri.conf.json"
$repo = "ncuong0704/MeetingOne"

# Update version
$config = Get-Content $configPath -Raw | ConvertFrom-Json
$config.version = $Version
$config | ConvertTo-Json -Depth 10 | Set-Content $configPath -Encoding utf8

Write-Host "Version updated to $Version"

# Commit and push
git add $configPath
git commit -m "chore: bump version to $Version"
git push

# Trigger release workflow
gh workflow run release.yml --repo $repo
Write-Host "Release workflow triggered. Monitor at: https://github.com/$repo/actions"
Write-Host ""
Write-Host "When build completes, publish with:"
Write-Host "  gh release edit v$Version --repo $repo --draft=false"
