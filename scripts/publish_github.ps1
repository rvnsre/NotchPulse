[CmdletBinding()]
param(
    [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9-]{0,38}$')]
    [string]$Owner = 'rvnsre',
    [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9._-]{0,99}$')]
    [string]$Name = 'NotchPulse'
)

$ErrorActionPreference = 'Stop'
$ProjectRoot = Split-Path -Parent $PSScriptRoot
foreach ($Required in @('git', 'gh')) {
    if (-not (Get-Command $Required -ErrorAction SilentlyContinue)) {
        throw "$Required is required for authenticated GitHub publication."
    }
}

Push-Location -LiteralPath $ProjectRoot
try {
    $Changes = & git status --porcelain
    if ($LASTEXITCODE -ne 0) { throw 'This project is not a Git repository.' }
    if ($Changes) { throw 'Commit the reviewed project changes before publication.' }
    $Branch = & git branch --show-current
    if ($LASTEXITCODE -ne 0 -or $Branch -ne 'main') { throw 'Publish the reviewed main branch only.' }

    $Login = & gh api user --jq '.login'
    if ($LASTEXITCODE -ne 0) { throw 'GitHub CLI authentication is unavailable.' }
    if ($Login -ne $Owner) { throw "Authenticated account does not match $Owner." }

    $Slug = "$Owner/$Name"
    $ExpectedRemote = "https://github.com/$Slug.git"
    $ExistingRemote = & git config --get remote.origin.url
    if ($ExistingRemote -and $ExistingRemote -ne $ExpectedRemote) {
        throw 'An unrelated origin is configured; publication was stopped.'
    }
    $RepositoryJson = & gh repo view $Slug --json nameWithOwner,visibility 2>$null
    if ($LASTEXITCODE -ne 0) {
        # New repositories stay private. No existing repository is overwritten.
        & gh repo create $Slug --private
        if ($LASTEXITCODE -ne 0) { throw 'GitHub repository creation failed.' }
    } else {
        $Repository = $RepositoryJson | ConvertFrom-Json
        if ($Repository.nameWithOwner -ne $Slug -or $Repository.visibility -ne 'PRIVATE') {
            throw 'Expected a matching private repository. Access settings were not changed.'
        }
    }
    if (-not $ExistingRemote) {
        & git remote add origin $ExpectedRemote
        if ($LASTEXITCODE -ne 0) { throw 'Could not configure origin.' }
    }
    & git push --set-upstream origin main
    if ($LASTEXITCODE -ne 0) { throw 'Push failed; no force-push was attempted.' }
    Write-Output "Published https://github.com/$Slug"
} finally {
    Pop-Location
}
