<#
.SYNOPSIS
    Demonstrates the Vitals -> Home Assistant MQTT integration against a real
    Home Assistant, and prints the entities Home Assistant created.

.DESCRIPTION
    1.3.0 shipped an MQTT discovery document that Home Assistant discarded in
    silence: `availability_topic`, `payload_available` and
    `payload_not_available` sat inside the document's `device` block, which
    Home Assistant validates strictly, so it threw away every document and
    created no entity. Vitals' own tests passed, the broker accepted the bytes,
    and the only place the rejection was visible was Home Assistant's own log
    inside its container. Fixed in e2a7f9b.

    This script recreates that environment so a person can verify it, not only
    CI:

      * an `eclipse-mosquitto` broker and a `home-assistant` container on a
        private Docker network, both pinned by digest;
      * Home Assistant onboarded over its REST API — no browser, no manual
        clicking — and the MQTT integration added the same way;
      * the Vitals publisher (`vitals serve --mqtt ...`) pointed at the broker;
      * the entities Home Assistant created, read back from **Home Assistant's
        API**. The broker proves delivery; only Home Assistant proves
        acceptance, and acceptance is what the defect broke.

        The entity ids are Home Assistant's own, and it builds them from the device
        name: the CPU reading becomes `sensor.vitals_on_dragos_cpu` while the
        `unique_id` Vitals sends is `vitals_dragos_cpu`. The script reads Home
        Assistant's entity registry rather than guessing at that name — a guess that
        finds nothing reports "no entity" for an instance that accepted everything —
        and prints the entity id, its state and the unique id it maps to.

    `-Reproduce` additionally publishes a discovery document shaped like the
    1.3.0 defect and prints Home Assistant's own rejection of it, so the failing
    case can be seen too. A demo that can only show the good case cannot be used
    to debug a bug report.

    CI already runs this proof on every change to `crates/vitals-mqtt/`:
    `.github/workflows/mqtt.yml`'s `mqtt-home-assistant` job boots the same two
    pinned containers, onboards Home Assistant the same way over REST, and lets
    `crates/vitals-mqtt/tests/home_assistant.rs` fail the build if no entity
    appears. So the question this script answers is not "does CI cover it" but
    "what does it look like, what did Home Assistant actually create, and what
    happens in the case CI cannot show". It is the same rig a person can watch,
    vary and keep.

.PARAMETER Keep
    Leave the containers running when the proof is done, and print the URL, the
    credentials and a link to the entity. Without it the run tears everything
    down, so a casual run leaves nothing behind.

.PARAMETER Down
    Remove the containers, the network and the volume this prefix owns, and say
    what was removed. Nothing is brought up. It removes only resources carrying
    this script's `vitals-demo` label: anything else with one of these names is
    reported and left alone, so `-Down` cannot destroy a rig somebody else
    started. `-Force` overrides that refusal.

.PARAMETER Reproduce
    After the good proof, publish a deliberately broken discovery document and
    print what Home Assistant said about it, plus the fact that it created no
    entity for it.

.PARAMETER Version
    Assert that the Vitals binary under test reports this exact version. The
    script demonstrates the behaviour of the binary it *finds*; it cannot fetch
    an arbitrary past release (this repository builds from source, and a demo
    must not download or compile), so `-Version` pins which build's behaviour is
    being shown rather than obtaining one. Use `-FromRelease` to run a published
    build.

.PARAMETER FromRelease
    Run the published CLI of this tag (`v1.3.0`, or `1.3.0`) as the publisher
    instead of the binary in `target/`. The `vitals-cli_<version>_<arch>.zip`
    asset is downloaded from this repository's GitHub releases and **verified
    against the release's own `SHA256SUMS.txt`** before it is run — this is the
    demo's only network call, and it is announced when it happens. `v1.3.0` is
    the build that shipped the defect, so `-FromRelease v1.3.0 -ExpectRejected`
    demonstrates the released binary failing, not a rebuild of the fixed tree.
    A second run reuses the verified download.

.PARAMETER ExpectRejected
    Assert the opposite outcome: the publisher under test is expected to be
    refused by Home Assistant, so no entity appearing is the run *succeeding*.
    This is what makes a shipped-defect demo usable as a proof rather than as a
    failure — pair it with `-FromRelease`. If an entity does appear, the run
    fails, because the binary is no longer broken the way it was believed to be.

.PARAMETER AssertAbsent
    Unique-id suffixes, without the node prefix, that this machine cannot
    measure — for example `battery,cpu_temperature` on a desktop. Each must
    exist in Home Assistant's registry and read `unknown`, never a number: an
    unmeasured reading is absent, and a `0` there would be a fabricated value
    that looks like a measurement. Off by default because it is machine
    dependent: on a laptop, `battery` is a real reading and this would fail.

.PARAMETER Json
    Write one JSON object describing the outcome to the success stream, so a
    person can redirect it (`> report.json`) and a machine can assert on it. The
    human-readable progress goes to the host and is *not* part of that stream.

.PARAMETER Force
    With `-Down`, also remove same-named resources this script did not create.
    Only for deliberately clearing a rig started by hand; without it `-Down`
    refuses to touch them.

.PARAMETER Prefix
    Names every resource `<Prefix>-ha`, `<Prefix>-mqtt`, `<Prefix>-ha-net` and
    `<Prefix>-ha-config`. Useful for an isolated run beside an existing rig; the
    default gives the names this repository documents.

.PARAMETER HaPort
    Host port Home Assistant is published on. Change it when something already
    holds 8123 — another Home Assistant, or a second run of this demo.

.PARAMETER MqttPort
    Host port the broker is published on, which is where the Vitals side
    connects. Change it for the same reason as `-HaPort`.

.EXAMPLE
    pwsh -NoProfile -File scripts/ha-demo.ps1
    Prove the integration works, then tear everything down.

.EXAMPLE
    pwsh -NoProfile -File scripts/ha-demo.ps1 -Keep -Reproduce
    Leave it running, and also show Home Assistant rejecting the 1.3.0 shape.

.EXAMPLE
    pwsh -NoProfile -File scripts/ha-demo.ps1 -FromRelease v1.3.0 -ExpectRejected
    Download the published 1.3.0 CLI, verify its checksum, and show it being
    refused: the released build carried the defect.

.EXAMPLE
    pwsh -NoProfile -File scripts/ha-demo.ps1 -Json -AssertAbsent battery,cpu_temperature > report.json
    Prove the integration, assert the absent readings read `unknown` rather
    than zero, and leave a machine-readable report on stdout.
#>
[CmdletBinding()]
param(
    [switch]$Keep,
    [switch]$Down,
    [switch]$Reproduce,
    [string]$Version,
    [string]$FromRelease,
    [switch]$ExpectRejected,
    [string[]]$AssertAbsent = @(),
    [switch]$Json,
    [switch]$Force,
    [string]$Prefix = 'vitals',
    [int]$HaPort = 8123,
    [int]$MqttPort = 11883,
    [int]$BootTimeoutSec = 300
)

$ErrorActionPreference = 'Stop'

# `pwsh -File scripts/ha-demo.ps1 -AssertAbsent battery,cpu_temperature` hands
# this parameter ONE string rather than a two-element array: a `-File` command
# line does not split on commas the way an interactive prompt does. Splitting here
# means the documented spelling works, and so does repeating the parameter or
# handing it a real array from PowerShell.
$AssertAbsent = @($AssertAbsent -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ })

# With -Json, stdout has to carry exactly one JSON document, so that
# `> report.json` produces something a caller can parse. Write-Host does not leave
# that possible: measured on PowerShell 7.6, a redirected `pwsh -File` run
# captures Write-Host output on stdout next to the success stream. So under -Json
# the narration goes to the error stream instead — still visible in a terminal and
# in a CI log, no longer inside the document a caller redirects.
#
# A wrapper rather than fifty rewritten call sites, and defined only when -Json is
# asked for, so an ordinary run takes exactly the path it took before.
if ($Json) {
    function Write-Host {
        param([Parameter(Position = 0)]$Object = '', [System.ConsoleColor]$ForegroundColor)
        [Console]::Error.WriteLine([string]$Object)
    }
}

if ($Keep -and $Down) { throw 'pick one: -Keep leaves the containers up, -Down removes them' }
# `-Reproduce` publishes a broken document and expects it to be refused, which
# needs a publisher that works. `-ExpectRejected` is the opposite: the publisher
# itself is the thing being refused. Together they contradict, and the second
# would fail with a misleading "expected a retained document, found none".
if ($Reproduce -and $ExpectRejected) {
    throw "pick one: -Reproduce shows Home Assistant refusing a broken document from a working publisher, -ExpectRejected expects the publisher itself to be refused"
}

# `-FromRelease` names the version under test, so `-Version` is redundant with it
# and only a contradiction when it disagrees. Settled here, before a two-minute
# boot has been spent.
$releaseVersion = $null
if ($FromRelease) {
    $releaseVersion = $FromRelease -replace '^v', ''
    if ($Version -and $Version -ne $releaseVersion) {
        throw "-Version $Version was asked for, but -FromRelease $FromRelease is the $releaseVersion build"
    }
    $Version = $releaseVersion
}

$root = Split-Path -Parent $PSScriptRoot
$haName = "$Prefix-ha"
$mqttName = "$Prefix-mqtt"
$netName = "$Prefix-ha-net"
$volName = "$Prefix-ha-config"

# Home Assistant's defaults. The discovery prefix is what Home Assistant
# subscribes to; the topic prefix is what Vitals publishes to. Both are the
# crate's defaults, so the demo exercises the same paths a user gets.
$discoveryPrefix = 'homeassistant'
$topicPrefix = 'vitals'

# Pinned by digest, read from the local images with `docker inspect` on
# 2026-11-20. The Home Assistant digest is the one the committed discovery-key
# fixture was enumerated from (`enumerated_with` in
# crates/vitals-mqtt/tests/fixtures/ha_discovery_keys.json, Home Assistant
# 2026.11.2), so pinning it keeps this demo and that fixture describing the same
# Home Assistant. A digest is immutable; `:stable` is not.
$haImage = 'ghcr.io/home-assistant/home-assistant@sha256:493a07e8f6530fc640223f4a0a5248f20a61ca2f718bb1d31efbf08a2c5ea28f'
$mqttImage = 'eclipse-mosquitto@sha256:fa12172a051310efe5bcb7a6a11b77bd5faacb82cbacd7742aa758bf024fd8e4'

# Every container, network and volume this script creates carries this label,
# which is what lets `-Down` tell its own rig from a rig somebody else started
# under the same names. Names alone cannot: `vitals-ha` is exactly what a person
# following the integration docs would call their Home Assistant container, so
# matching on the name would mean a default-prefix `-Down` silently destroying
# their instance.
$demoLabel = 'vitals-demo'

# The Vitals headless server always binds a LAN API port even though nothing in
# this demo talks to it. The number is chosen at run time from a port the OS
# hands out: Windows reserves whole ranges for Hyper-V, and a hard-coded port
# there fails with `os error 10013` before a single reading is published.

# How long to wait for the Vitals publisher's first reading to reach Home
# Assistant. Discovery is immediate; the first reading follows the interval.
$entityTimeoutSec = 60

# ---------------------------------------------------------------- utilities

function Write-Step([string]$Text) {
    Write-Host ''
    Write-Host "== $Text" -ForegroundColor Cyan
}

function Assert-Docker {
    if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
        throw 'docker was not found on PATH; this demo needs Docker Desktop'
    }
    docker info *> $null
    if ($LASTEXITCODE -ne 0) { throw 'the Docker engine is not answering `docker info`' }
}

function Test-ContainerExists([string]$Name) {
    $found = @(docker ps -a --filter "name=^/$Name$" --format '{{.Names}}' 2>$null)
    return [bool]($found -contains $Name)
}

function Test-NetworkExists([string]$Name) {
    docker network inspect $Name *> $null
    return ($LASTEXITCODE -eq 0)
}

function Test-VolumeExists([string]$Name) {
    docker volume inspect $Name *> $null
    return ($LASTEXITCODE -eq 0)
}

# The labels on a resource, or $null when it carries none. Read as JSON rather
# than by indexing one key, because Docker prints an empty string for a missing
# key and `{{json ...}}` distinguishes "no labels" from "no such label".
function Get-ResourceLabels([string]$Kind, [string]$Name) {
    # Containers keep labels under `.Config`; volumes and networks at the top
    # level. Indexing the wrong one returns nothing at all, which would read as
    # "unlabelled" for a resource this script created.
    $format = if ($Kind -eq 'container') { '{{json .Config.Labels}}' } else { '{{json .Labels}}' }
    $raw = switch ($Kind) {
        'container' { docker inspect --format $format $Name 2>$null }
        'volume' { docker volume inspect --format $format $Name 2>$null }
        default { docker network inspect --format $format $Name 2>$null }
    }
    if (-not $raw -or $raw -eq 'null') { return $null }
    return ($raw | ConvertFrom-Json)
}

# Whether this script created the resource. Anything else — including a rig a
# person started by hand under the same name — is theirs, and `-Down` reports it
# instead of removing it.
function Test-DemoOwned([string]$Kind, [string]$Name) {
    $labels = Get-ResourceLabels -Kind $Kind -Name $Name
    if ($null -eq $labels) { return $false }
    return ($null -ne $labels.PSObject.Properties[$demoLabel])
}

function Assert-ImagePresent([string]$Image) {
    docker image inspect $Image *> $null
    if ($LASTEXITCODE -ne 0) {
        # Said plainly rather than silently pulled: the Home Assistant image is
        # over 2 GB, and a demo should not start a large download unannounced.
        throw "the pinned image $Image is not present locally; run `docker pull $Image` yourself, this demo never pulls"
    }
}

# A URL-safe secret, from the platform CSPRNG. Generated per run and printed
# once: a credential that ships in the file is not a credential.
function New-RandomSecret([int]$Bytes = 18) {
    $buffer = [byte[]]::new($Bytes)
    [System.Security.Cryptography.RandomNumberGenerator]::Fill($buffer)
    return ([Convert]::ToBase64String($buffer) -replace '\+', '-' -replace '/', '_').TrimEnd('=')
}

# A port the OS is willing to hand out, so the Vitals server can bind one
# without a reserved-range collision. Binding to port 0 asks the kernel for a
# free ephemeral port and reads back which one it chose.
function Get-FreePort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $port = $listener.LocalEndpoint.Port
    $listener.Stop()
    return $port
}

# The topic segment Vitals derives from the host name, so the demo can look up
# the entities without being told where they are. Mirrors `topics::node_slug`.
function Get-NodeSlug {
    $host_ = if ($env:COMPUTERNAME) { $env:COMPUTERNAME } elseif ($env:HOSTNAME) { $env:HOSTNAME } else { 'machine' }
    $slug = ($host_.ToLowerInvariant().ToCharArray() | ForEach-Object {
            if ([char]::IsAsciiLetterOrDigit($_)) { $_ } else { '_' }
        }) -join ''
    if (-not ($slug.ToCharArray() | Where-Object { $_ -ne '_' })) { $slug = 'machine' }
    return "vitals_$slug"
}

function Resolve-VitalsBinary {
    if ($env:VITALS_BIN) {
        if (-not (Test-Path -LiteralPath $env:VITALS_BIN)) { throw "VITALS_BIN points at $env:VITALS_BIN, which does not exist" }
        return (Resolve-Path -LiteralPath $env:VITALS_BIN).Path
    }
    foreach ($candidate in @('target/release/vitals.exe', 'target/release/vitals', 'target/debug/vitals.exe', 'target/debug/vitals')) {
        $path = Join-Path $root $candidate
        if (Test-Path -LiteralPath $path) { return $path }
    }
    throw 'no built Vitals CLI found; build one with `cargo build -p vitals-cli` (release recommended) or set VITALS_BIN'
}

# The published CLI of a tag, so the demo can show what a *shipped* build does
# rather than a rebuild of the current tree. The `vitals-cli_*.zip` asset is the
# one used because `vitals serve` is what publishes; the installers exist too and
# would have to be installed to get at the same executable.
#
# The download is verified against the release's own `SHA256SUMS.txt` before it
# is run. A demo of "the released build is broken" is worth nothing if the bytes
# are not the released build, and a truncated or substituted asset would produce
# exactly the same story as the defect.
function Resolve-ReleaseBinary([string]$Tag) {
    $repoSlug = 'dragoscv/vitals'
    $normalised = if ($Tag -match '^v') { $Tag } else { "v$Tag" }
    $arch = if ("$([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture)" -eq 'Arm64') { 'arm64' } else { 'x64' }
    $dir = Join-Path (Join-Path $env:TEMP 'vitals-ha-demo') "release-$normalised"
    $binary = Join-Path $dir 'extracted/vitals.exe'
    if (Test-Path -LiteralPath $binary) {
        Write-Host "reusing the verified $normalised CLI from $dir"
        return $binary
    }

    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    Write-Host "downloading the published $normalised CLI - this is the demo's only network call"
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repoSlug/releases/tags/$normalised" `
        -Headers @{ Accept = 'application/vnd.github+json' } -TimeoutSec 60
    $asset = @($release.assets | Where-Object { $_.name -match "^vitals-cli_.*_$arch\.zip$" })[0]
    if (-not $asset) {
        throw "$normalised publishes no vitals-cli_*_$arch.zip; it has: $(($release.assets | ForEach-Object { $_.name }) -join ', ')"
    }
    $sumsAsset = @($release.assets | Where-Object { $_.name -eq 'SHA256SUMS.txt' })[0]
    if (-not $sumsAsset) {
        throw "$normalised publishes no SHA256SUMS.txt, so the download cannot be verified; refusing to run unchecked bytes"
    }

    $zip = Join-Path $dir $asset.name
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zip -TimeoutSec 300
    $sums = Join-Path $dir 'SHA256SUMS.txt'
    Invoke-WebRequest -Uri $sumsAsset.browser_download_url -OutFile $sums -TimeoutSec 60
    $line = @(Select-String -Path $sums -Pattern ([regex]::Escape($asset.name)))[0]
    if (-not $line) { throw "$normalised's SHA256SUMS.txt lists no $($asset.name)" }
    $want = ($line.Line -split '\s+')[0].ToLowerInvariant()
    $got = (Get-FileHash -Algorithm SHA256 -Path $zip).Hash.ToLowerInvariant()
    if ($want -ne $got) { throw "$($asset.name) downloaded as sha256 $got, but the release says $want" }
    Write-Host "verified $($asset.name) against the release checksum ($($got.Substring(0, 12))...)"

    $extracted = Join-Path $dir 'extracted'
    if (Test-Path -LiteralPath $extracted) { Remove-Item -Recurse -Force -LiteralPath $extracted }
    Expand-Archive -Path $zip -DestinationPath $extracted
    if (Test-Path -LiteralPath $binary) { return $binary }
    $found = @(Get-ChildItem -Recurse -Path $extracted -Filter 'vitals*' | Where-Object { -not $_.PSIsContainer })[0]
    if (-not $found) { throw "$($asset.name) holds no Vitals binary" }
    return $found.FullName
}

# ------------------------------------------------------------ docker lifecyle

# What the last teardown removed, for `-Down -Json`. Reset per call so a caller
# cannot read a previous run's list.
$script:demoRemoved = [System.Collections.Generic.List[string]]::new()

function Remove-DemoResources {
    $script:demoRemoved = [System.Collections.Generic.List[string]]::new()
    $removed = [System.Collections.Generic.List[string]]::new()
    $leftAlone = [System.Collections.Generic.List[string]]::new()

    # Ownership is checked per resource, before removing it, and a resource that
    # is not ours is named rather than deleted. `-Force` is the deliberate way to
    # clear a rig this script did not make.
    $check = {
        param($Kind, $Name, $Exists)
        if (-not $Exists) { return $false }
        if ($Force -or (Test-DemoOwned -Kind $Kind -Name $Name)) { return $true }
        $leftAlone.Add("$Kind $Name")
        return $false
    }

    foreach ($container in @($haName, $mqttName)) {
        if (& $check 'container' $container (Test-ContainerExists $container)) {
            docker rm -f $container *> $null
            $removed.Add("container $container")
        }
    }
    if (& $check 'network' $netName (Test-NetworkExists $netName)) {
        docker network rm $netName *> $null
        $removed.Add("network $netName")
    }
    if (& $check 'volume' $volName (Test-VolumeExists $volName)) {
        docker volume rm $volName *> $null
        $removed.Add("volume $volName (Home Assistant's config and database)")
    }

    foreach ($item in $leftAlone) {
        Write-Host "left alone: $item carries no '$demoLabel' label, so this script did not create it" -ForegroundColor Yellow
    }
    if ($leftAlone.Count -gt 0 -and -not $Force) {
        # Loud rather than silent: a teardown that removed nothing because it was
        # not allowed to must not look like a teardown that removed nothing
        # because there was nothing there.
        throw "$($leftAlone.Count) resource(s) with this prefix were not created by this script and were left alone; remove them yourself, or pass -Force to remove them anyway"
    }

    if ($removed.Count -eq 0) {
        Write-Host "nothing to remove: no $haName, $mqttName, $netName or $volName exists"
    }
    else {
        foreach ($item in $removed) { Write-Host "removed $item" }
        $script:demoRemoved.AddRange($removed)
    }
}

function New-Broker {
    # `allow_anonymous` and no persistence: a throwaway broker for a demo, whose
    # retained messages should vanish when the container does. Binding to
    # 0.0.0.0 inside the container is what lets Home Assistant reach it by name
    # on the private network; the host only sees the published port.
    $scratch = Join-Path $env:TEMP 'vitals-ha-demo'
    New-Item -ItemType Directory -Force -Path $scratch | Out-Null
    $confPath = Join-Path $scratch 'mosquitto.conf'
    Set-Content -LiteralPath $confPath -Value "listener 1883`nallow_anonymous true`npersistence false`n" -NoNewline -Encoding ascii

    docker run -d --name $mqttName --network $netName --label "$demoLabel=1" -p "${MqttPort}:1883" `
        -v "${confPath}:/mosquitto/config/mosquitto.conf:ro" `
        $mqttImage *> $null
    if ($LASTEXITCODE -ne 0) { throw "the broker container failed to start" }
}

function New-HomeAssistant {
    docker run -d --name $haName --network $netName --label "$demoLabel=1" -p "${HaPort}:8123" `
        -v "${volName}:/config" `
        $haImage *> $null
    if ($LASTEXITCODE -ne 0) { throw "the Home Assistant container failed to start" }
}

# ------------------------------------------------------- Home Assistant API

function Wait-ForHomeAssistant([string]$Base, [int]$TimeoutSec) {
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        try {
            # Element by element for the same reason as Invoke-HaJson: a JSON
            # array arrives as one object, and the caller filters these steps.
            foreach ($step in Invoke-RestMethod -Method Get -Uri "$Base/api/onboarding" -TimeoutSec 10) { $step }
            return
        }
        catch { Start-Sleep -Seconds 2 }
    }
    # Not a count of anything: Home Assistant never answered, which is a
    # different fact from "answered with nothing".
    throw "Home Assistant did not answer $Base/api/onboarding within $TimeoutSec s"
}

function Get-HaToken([string]$Base, [string]$User, [string]$Password) {
    $login = Invoke-RestMethod -Method Post -Uri "$Base/auth/login_flow" -ContentType 'application/json' -Body (@{
            client_id   = "$Base/"
            handler     = @('homeassistant', $null)
            redirect_uri = "$Base/?auth_callback=1"
            type        = 'login_flow'
        } | ConvertTo-Json)
    $step = Invoke-RestMethod -Method Post -Uri "$Base/auth/login_flow/$($login.flow_id)" -ContentType 'application/json' -Body (@{
            username = $User; password = $Password; client_id = "$Base/"
        } | ConvertTo-Json)
    if ($step.type -ne 'create_entry') {
        throw "the Home Assistant login flow ended as '$($step.type)'; the stored credentials do not match this instance"
    }
    # Form-encoded, not JSON: Home Assistant's token endpoint rejects a JSON
    # body, which is one of the two contracts that had to be discovered first.
    $token = Invoke-RestMethod -Method Post -Uri "$Base/auth/token" -ContentType 'application/x-www-form-urlencoded' -Body @{
        grant_type = 'authorization_code'; code = $step.result; client_id = "$Base/"
    }
    return $token.access_token
}

function Invoke-HaJson([string]$Base, [string]$Path, [string]$Token, [string]$Method = 'Get', $Body = $null) {
    $headers = @{ Authorization = "Bearer $Token" }
    $arguments = @{ Method = $Method; Uri = "$Base$Path"; Headers = $headers; TimeoutSec = 30 }
    if ($null -ne $Body) {
        $arguments['ContentType'] = 'application/json'
        $arguments['Body'] = ($Body | ConvertTo-Json -Depth 8)
    }
    $response = Invoke-RestMethod @arguments
    # One item per element, deliberately. `Invoke-RestMethod` hands a JSON array
    # back as a single object rather than as elements, so a caller doing
    # `@(...) | Where-Object { $_.domain -eq 'mqtt' }` sees one item whose
    # `.domain` is the array of every domain: the filter then matches whenever
    # *any* entry matches, and a fresh instance reads as fully configured.
    # Looping here means every caller in this script can loop in the obvious way.
    foreach ($item in $response) { $item }
}

function Set-MqttEntry([string]$Base, [string]$Token, [string]$Broker, [int]$Port) {
    $existing = @(Invoke-HaJson -Base $Base -Path '/api/config/config_entries/entry' -Token $Token)
    $already = @($existing | Where-Object { $_.domain -eq 'mqtt' -and $_.state -eq 'loaded' })
    if ($already.Count -gt 0) {
        Write-Host "the MQTT integration is already set up (entry '$($already[0].title)')"
        return
    }
    $flow = Invoke-HaJson -Base $Base -Path '/api/config/config_entries/flow' -Token $Token -Method 'Post' -Body @{ handler = 'mqtt' }
    # One step carries every field, including the collapsed `other_settings`
    # block, which is required even though it looks optional in the form.
    $result = Invoke-HaJson -Base $Base -Path "/api/config/config_entries/flow/$($flow.flow_id)" -Token $Token -Method 'Post' -Body @{
        broker         = $Broker
        port           = $Port
        protocol       = '3.1.1'
        other_settings = @{ set_client_cert = $false; set_ca_cert = 'off' }
    }
    if ($result.type -ne 'create_entry') {
        throw "the MQTT config-entry flow ended as '$($result.type)' with errors: $($result.errors | ConvertTo-Json -Compress)"
    }
    Write-Host "added the MQTT integration, pointing at $Broker`:$Port"
}

# `/api/onboarding` and the config-entry flows come up before the state machine
# does, so a token that works for onboarding can still get a 503 from
# `/api/states`. Wait for the endpoint the whole proof depends on, and name the
# last error if it never answers rather than failing later with "no entity".
function Wait-ForStatesApi([string]$Base, [string]$Token, [int]$TimeoutSec) {
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    $lastError = 'no response yet'
    while ((Get-Date) -lt $deadline) {
        try {
            Invoke-HaJson -Base $Base -Path '/api/states' -Token $Token | Out-Null
            return
        }
        catch {
            $lastError = $_.Exception.Message
            Start-Sleep -Seconds 2
        }
    }
    throw "Home Assistant's /api/states never answered within $TimeoutSec s (last error: $lastError)"
}

# ------------------------------------------------- Home Assistant registry

# The entity registry is the mapping from the `unique_id` Vitals publishes to
# the `entity_id` Home Assistant chose, and it is WebSocket-only: there is no
# REST route for it (`/api/config/entity_registry/list` answers 404). It is also
# the only correct way to find the entities. Home Assistant builds the entity id
# from the *device name*, not from the `object_id` Vitals sends:
#
#   unique_id vitals_dragos_cpu  ->  entity_id sensor.vitals_on_dragos_cpu
#   ("Vitals on DRAGOS" + "CPU"), while object_id is `vitals_dragos_cpu`
#
# so matching entity ids on a substring of the node slug finds nothing and
# reports "no entity" for a Home Assistant that accepted every document — the
# exact false alarm this registry lookup replaces. Asking Home Assistant reports
# what it actually built instead of re-implementing its naming rules and
# silently drifting from them.

function Receive-HaWsMessage([System.Net.WebSockets.ClientWebSocket]$Socket, [int]$TimeoutSec = 10) {
    $buffer = [byte[]]::new(65536)
    $segment = [ArraySegment[byte]]::new($buffer)
    $builder = [System.Text.StringBuilder]::new()
    $cancel = [System.Threading.CancellationTokenSource]::new()
    $cancel.CancelAfter([TimeSpan]::FromSeconds($TimeoutSec))
    try {
        do {
            $received = $Socket.ReceiveAsync($segment, $cancel.Token).GetAwaiter().GetResult()
            [void]$builder.Append([System.Text.Encoding]::UTF8.GetString($buffer, 0, $received.Count))
        } while (-not $received.EndOfMessage)
    }
    catch [System.OperationCanceledException] {
        throw "Home Assistant's WebSocket sent nothing for $TimeoutSec s"
    }
    finally { $cancel.Dispose() }
    return ($builder.ToString() | ConvertFrom-Json)
}

function Send-HaWsMessage([System.Net.WebSockets.ClientWebSocket]$Socket, $Message) {
    $bytes = [System.Text.Encoding]::UTF8.GetBytes(($Message | ConvertTo-Json -Compress -Depth 8))
    $segment = [ArraySegment[byte]]::new($bytes)
    $Socket.SendAsync($segment, [System.Net.WebSockets.WebSocketMessageType]::Text, $true, [System.Threading.CancellationToken]::None).GetAwaiter().GetResult() | Out-Null
}

function Connect-HaWebSocket([string]$Base, [string]$Token) {
    $socket = [System.Net.WebSockets.ClientWebSocket]::new()
    $cancel = [System.Threading.CancellationTokenSource]::new()
    $cancel.CancelAfter([TimeSpan]::FromSeconds(10))
    try {
        # `| Out-Null` is load-bearing: without it the await helper's
        # VoidTaskResult is written to this function's output stream, so the
        # socket comes back as a two-element array and every later call fails
        # for a reason that has nothing to do with Home Assistant.
        $socket.ConnectAsync([Uri](($Base -replace '^http', 'ws') + '/api/websocket'), $cancel.Token).GetAwaiter().GetResult() | Out-Null
    }
    catch {
        $socket.Dispose()
        throw "could not open Home Assistant's WebSocket at $Base/api/websocket: $($_.Exception.Message)"
    }
    finally { $cancel.Dispose() }

    $hello = Receive-HaWsMessage $socket
    $auth = $null
    if ($hello.type -eq 'auth_required') {
        Send-HaWsMessage $socket @{ type = 'auth'; access_token = $Token }
        $auth = Receive-HaWsMessage $socket
    }
    if ($hello.type -ne 'auth_required' -or $auth.type -ne 'auth_ok') {
        $socket.Dispose()
        $seen = if ($null -ne $auth) { $auth.type } else { $hello.type }
        throw "Home Assistant's WebSocket did not authenticate ('$seen'; expected auth_required then auth_ok)"
    }
    return $socket
}

# One request/response on the socket. Home Assistant may interleave messages
# that carry another id, so wait for the reply to ours rather than the next one.
# The id must be new every time: re-sending one it has already answered gets
# `success: false` and no result, which looks exactly like a refusal.
function Get-HaWsResult([System.Net.WebSockets.ClientWebSocket]$Socket, [string]$Type) {
    $script:haWsMessageId++
    $id = $script:haWsMessageId
    Send-HaWsMessage $Socket @{ id = $id; type = $Type }
    for ($attempt = 0; $attempt -lt 10; $attempt++) {
        $message = Receive-HaWsMessage $Socket
        if ($message.id -ne $id) { continue }
        if (-not $message.success) {
            throw "Home Assistant refused '$Type': $($message.error | ConvertTo-Json -Compress)"
        }
        return $message.result
    }
    throw "Home Assistant never answered '$Type' (ten messages carried another id); the WebSocket may have been replaced by a Home Assistant restart"
}

# The Vitals entities Home Assistant holds, each joined with its current state.
# Returns an object rather than a bare array because PowerShell unrolls a
# returned empty array to $null, which would make "Home Assistant created
# nothing" indistinguishable from "Home Assistant did not answer".
function Get-VitalsEntities {
    param(
        [string]$Base,
        [string]$Token,
        [System.Net.WebSockets.ClientWebSocket]$Socket,
        [string]$UniqueIdPrefix
    )
    $registry = @(Get-HaWsResult -Socket $Socket -Type 'config/entity_registry/list')
    $states = @(Invoke-HaJson -Base $Base -Path '/api/states' -Token $Token)
    $stateByEntityId = @{}
    foreach ($state in $states) { $stateByEntityId[$state.entity_id] = $state }

    $entities = [System.Collections.Generic.List[object]]::new()
    foreach ($entry in $registry) {
        # `platform -eq 'mqtt'` keeps this to the integration under test; the
        # unique_id prefix keeps it to the node or document being looked for.
        if ($entry.platform -ne 'mqtt') { continue }
        if (-not ("$($entry.unique_id)" -like "$UniqueIdPrefix*")) { continue }
        $state = $stateByEntityId[$entry.entity_id]
        $entities.Add([pscustomobject]@{
                EntityId = $entry.entity_id
                UniqueId = $entry.unique_id
                # $null: Home Assistant registered the entity but has published
                # no state for it, which is not a state of zero.
                State    = if ($null -ne $state) { $state.state } else { $null }
            # The unit and class come from Home Assistant's copy of the
            # discovery document, not from Vitals, so a report built from
            # these is evidence about what the consumer understood.
            Unit         = if ($null -ne $state) { $state.attributes.unit_of_measurement } else { $null }
            DeviceClass  = if ($null -ne $state) { $state.attributes.device_class } else { $null }
            })
    }
    return [pscustomobject]@{ Entities = $entities.ToArray() }
}

# The publisher's own stderr is the reason when it dies before publishing, but
# it also prints the generated LAN token, so token-bearing lines are dropped
# before anything is shown.
function Show-VitalsStderr {
    if (-not (Test-Path -LiteralPath $vitalsErrLog)) { return }
    $lines = @(Get-Content -LiteralPath $vitalsErrLog -ErrorAction SilentlyContinue |
        Where-Object { $_ -notmatch '(?i)token|#t=|bearer' })
    if ($lines.Count -eq 0) { return }
    Write-Host 'the Vitals publisher said (lines carrying its token removed):' -ForegroundColor Yellow
    foreach ($line in $lines) { Write-Host "  $line" }
}

# --------------------------------------------------------------- MQTT reads

# The retained payload on a topic, or $null when nothing is retained. A
# wildcard topic returns only the first match.
function Get-RetainedMessage([string]$Topic) {
    $out = @(docker exec $mqttName mosquitto_sub -h localhost -t $Topic -C 1 -W 4 2>$null)
    if ($out.Count -eq 0) { return $null }
    return ($out -join "`n")
}

function Publish-Retained([string]$Topic, [string]$Payload) {
    $scratch = Join-Path $env:TEMP 'vitals-ha-demo'
    New-Item -ItemType Directory -Force -Path $scratch | Out-Null
    $hostFile = Join-Path $scratch 'payload.json'
    # UTF-8 without a BOM: a unit like `°C` must survive, and a BOM would make
    # the payload invalid JSON to Home Assistant.
    Set-Content -LiteralPath $hostFile -Value $Payload -NoNewline -Encoding utf8NoBOM
    docker cp $hostFile "${mqttName}:/tmp/payload.json" *> $null
    docker exec $mqttName mosquitto_pub -h localhost -t $Topic -r -f /tmp/payload.json
    if ($LASTEXITCODE -ne 0) { throw "the broker refused a retained publish to $Topic" }
}

function Clear-Retained([string]$Topic) {
    docker exec $mqttName mosquitto_pub -h localhost -t $Topic -r -n *> $null
}

# Home Assistant logs to a file inside its own container, which `docker logs`
# never shows — the whole reason the defect stayed invisible. This is the only
# place the rejection appears.
function Get-HaLogLines([string]$Pattern, [int]$Tail = 12) {
    $lines = @(docker exec $haName sh -c "grep -a -E '$Pattern' /config/home-assistant.log 2>/dev/null | tail -$Tail" 2>$null)
    return $lines
}

# ------------------------------------------------------------------ proving

function Invoke-Reproduce([string]$Base, [string]$Token, [System.Net.WebSockets.ClientWebSocket]$Socket, [string]$Node) {
    Write-Step 'Reproduce the 1.3.0 defect'

    $cpuTopic = "$discoveryPrefix/sensor/$node/${node}_cpu/config"
    $good = Get-RetainedMessage $cpuTopic
    if (-not $good) { throw "expected a retained document on $cpuTopic, found none — the good run should have published it" }
    $document = $good | ConvertFrom-Json

    # Exactly the 1.3.0 mistake: the three availability keys copied into the
    # `device` block, where Home Assistant's device schema does not allow them.
    # A distinct topic and unique id keep the broken document off the real
    # entity, so the failure is additive and the good entity survives beside it.
    $reproObject = "${node}_reproduces_the_defect"
    $broken = [ordered]@{}
    foreach ($property in $document.PSObject.Properties) { $broken[$property.Name] = $property.Value }
    $broken['unique_id'] = $reproObject
    $broken['object_id'] = $reproObject
    $broken['name'] = 'CPU (1.3.0 shape)'
    $broken['device'] = [ordered]@{
        identifiers        = @($Node)
        manufacturer       = 'Vitals'
        name               = "Vitals on $Node"
        sw_version         = '1.3.0'
        availability_topic = "$topicPrefix/$node/availability"
        payload_available  = 'online'
        payload_not_available = 'offline'
    }
    $brokenTopic = "$discoveryPrefix/sensor/$node/$reproObject/config"
    Publish-Retained -Topic $brokenTopic -Payload ($broken | ConvertTo-Json -Depth 10 -Compress)

    # Bounded wait for Home Assistant to log its verdict, rather than a fixed
    # sleep that either wastes time or races it.
    $rejection = $null
    $deadline = (Get-Date).AddSeconds(20)
    while ((Get-Date) -lt $deadline) {
        $rejection = Get-HaLogLines "not a valid option|reproduces_the_defect" 6
        if ($rejection.Count -gt 0) { break }
        Start-Sleep -Seconds 1
    }

    # $null means Home Assistant did not answer; an empty registry result means
    # it read the document and created nothing, which is the defect's signature.
    $probe = $null
    try { $probe = Get-VitalsEntities -Base $Base -Token $Token -Socket $Socket -UniqueIdPrefix $reproObject }
    catch { $probe = $null }
    if ($null -eq $probe) {
        Write-Host 'Home Assistant did not answer while reproducing the defect, so the rejection could not be confirmed' -ForegroundColor Yellow
    }
    elseif (@($probe.Entities).Count -eq 0) {
        Write-Host 'Home Assistant created no entity for the broken document — as the defect predicts'
    }
    else {
        throw "Home Assistant created $(@($probe.Entities).Count) entity/entities for a document it should have rejected"
    }

    if ($rejection.Count -gt 0) {
        Write-Host 'Home Assistant said, in its own log:' -ForegroundColor Yellow
        foreach ($line in $rejection) { Write-Host "  $line" }
    }
    else {
        Write-Host 'Home Assistant logged no rejection line within 20 s (it may have changed its wording); the missing entity above is still the verdict' -ForegroundColor Yellow
    }

    Clear-Retained $brokenTopic
    Write-Host 'cleared the broken retained document'
    # Returned rather than only printed, so `-Json` reports the reproduction as a
    # fact with three distinct outcomes instead of a line of prose: answered with
    # no entity (the defect), answered with an entity (the defect is gone), or
    # never answered (unproven).
    return [pscustomobject]@{
        answered          = ($null -ne $probe)
        rejected          = ($rejection.Count -gt 0)
        entityCreated     = ($null -ne $probe -and @($probe.Entities).Count -gt 0)
        homeAssistantSaid = @($rejection)
    }
}

# The machine-readable half of the proof. `-Json` exists so something other than
# a person can assert on the outcome and compare two builds' entity sets, which
# means one object with a stable shape rather than a table to be scraped.
#
# Written to the success stream deliberately, and it is the only thing on that
# stream because `-Json` sends the progress commentary to the error stream (see
# the note beside the parameter block). `mode` is part of the contract because a
# teardown has no acceptance to report, and inventing an `accepted` for it would
# be a value about nothing.
function Write-JsonResult {
    param(
        [string]$Mode,
        [bool]$Ok,
        [string]$Node,
        [bool]$Accepted,
        [string]$Expected,
        [string]$Error,
        [string]$EntityLink,
        $Entities = @(),
        $Reproduce = $null,
        $Removed = $null
    )
    $document = [ordered]@{ mode = $Mode; ok = $Ok }
    if ($Mode -eq 'teardown') {
        # Assigned rather than produced by an `if` expression: an empty array
        # coming out of an expression unrolls to $null, which serialises as
        # `"removed": null` and reads as "the list is unknown" instead of "nothing
        # was removed". A direct assignment of `@()` keeps the empty array.
        $document['removed'] = @()
        if ($null -ne $Removed) { $document['removed'] = @($Removed) }
    }
    else {
        $document['node'] = $Node
        $document['expected'] = $Expected
        $document['accepted'] = $Accepted
        $document['entityLink'] = $EntityLink
        $document['entities'] = @($Entities)
        if ($null -ne $Reproduce) { $document['reproduce'] = $Reproduce }
        if (-not $Ok) { $document['error'] = $Error }
    }
    Write-Output ($document | ConvertTo-Json -Depth 8)
}

# --------------------------------------------------------------------- main

$base = "http://127.0.0.1:$HaPort"
$node = Get-NodeSlug
$haUser = 'vitals'
$haPassword = $null
# Every WebSocket request carries the next id; Home Assistant rejects a repeat.
$haWsMessageId = 0

if ($Down) {
    try {
        Assert-Docker
        Write-Step "Removing $Prefix resources"
        Remove-DemoResources
    }
    catch {
        # A refusal is an outcome, and it is reported the same way a failed proof
        # is: one line, its own words, and exit 1. A stack trace here would bury
        # the sentence that says what was left alone and why.
        if ($Json) { Write-JsonResult -Mode 'teardown' -Ok $false -Error $_.Exception.Message }
        Write-Host ''
        Write-Host "FAILED: $($_.Exception.Message)" -ForegroundColor Red
        exit 1
    }
    if ($Json) { Write-JsonResult -Mode 'teardown' -Ok $true -Removed $script:demoRemoved }
    exit 0
}

$vitalsProcess = $null
$vitalsErrLog = Join-Path (Join-Path $env:TEMP 'vitals-ha-demo') 'vitals.err'
$haSocket = $null
$entityLink = "http://127.0.0.1:$HaPort/config/entities"
$proofOk = $false
$failure = $null
# Collected as the proof proceeds, so `-Json` reports the same facts the console
# just showed, and a run that failed still says how far it got.
$entityReport = [System.Collections.Generic.List[object]]::new()
$reproduceReport = $null
$rejectedAsExpected = $false
# What Home Assistant did, tracked separately from whether that was the outcome
# that was asked for. A run can fail on a later assertion with the entities
# already created, and reporting `accepted: false` there would be a claim about
# Home Assistant drawn from the verdict instead of from the evidence.
$acceptedByHa = $false

try {
    Assert-Docker

    Write-Step "Preparing the $Prefix rig"
    # The rig needs resources that either do not exist or are ours. One that is
    # not ours means somebody else's instance is using this name — a person
    # following the integration docs would call theirs `vitals-ha` — and the only
    # safe thing to do with it is stop and say so.
    $foreign = [System.Collections.Generic.List[string]]::new()
    foreach ($container in @($haName, $mqttName)) {
        if (-not (Test-ContainerExists $container)) { continue }
        if (Test-DemoOwned -Kind 'container' -Name $container) {
            throw "a container named $container already exists, created by an earlier run of this script. Run -Down to remove it, or choose another -Prefix for an isolated run."
        }
        $foreign.Add("container $container")
    }
    if ((Test-NetworkExists $netName) -and -not (Test-DemoOwned -Kind 'network' -Name $netName)) { $foreign.Add("network $netName") }
    if ((Test-VolumeExists $volName) -and -not (Test-DemoOwned -Kind 'volume' -Name $volName)) { $foreign.Add("volume $volName") }
    if ($foreign.Count -gt 0) {
        throw "these already exist and were not created by this demo, so nothing will touch them: $($foreign -join ', '). Choose another -Prefix for an isolated run."
    }
    Assert-ImagePresent $haImage
    Assert-ImagePresent $mqttImage
    Write-Host 'both pinned images are present locally'

    if (-not (Test-NetworkExists $netName)) { docker network create --label "$demoLabel=1" $netName *> $null }
    docker volume create --label "$demoLabel=1" $volName *> $null
    New-Broker
    New-HomeAssistant
    Write-Host "started $mqttName and $haName on $netName"

    Write-Step 'Onboarding Home Assistant over its API'
    $onboarding = Wait-ForHomeAssistant -Base $base -TimeoutSec $BootTimeoutSec
    $userStep = @($onboarding | Where-Object { $_.step -eq 'user' })
    if ($userStep.Count -gt 0 -and $userStep[0].done) {
        Write-Host 'this instance is already onboarded; no credentials were generated'
        throw 'a freshly created volume should be un-onboarded; if you reused a volume, pass -Down first'
    }
    $haPassword = New-RandomSecret
    $created = Invoke-RestMethod -Method Post -Uri "$base/api/onboarding/users" -ContentType 'application/json' -Body (@{
            name = 'Vitals demo'; username = $haUser; password = $haPassword; client_id = "$base/"; language = 'en'
        } | ConvertTo-Json)
    Write-Host ''
    Write-Host 'Home Assistant credentials (shown once, generated this run):' -ForegroundColor Green
    Write-Host "  username: $haUser"
    Write-Host "  password: $haPassword"

    $token = Get-HaToken -Base $base -User $haUser -Password $haPassword
    Invoke-HaJson -Base $base -Path '/api/onboarding/core_config' -Token $token -Method 'Post' -Body @{} | Out-Null
    # Optional step; Home Assistant is usable whether or not it answers.
    try { Invoke-HaJson -Base $base -Path '/api/onboarding/analytics' -Token $token -Method 'Post' -Body @{} | Out-Null } catch { }

    Wait-ForStatesApi -Base $base -Token $token -TimeoutSec 120

    Write-Step 'Adding the MQTT integration'
    Set-MqttEntry -Base $base -Token $token -Broker $mqttName -Port 1883
    $haSocket = Connect-HaWebSocket -Base $base -Token $token

    Write-Step 'Publishing from the Vitals side'
    # The binary under test is named on purpose: a rebuild of the current tree is
    # the default so a working copy can be demoed, and `-FromRelease` swaps in the
    # published build so a shipped defect can be shown as shipped.
    $vitalsExe = if ($FromRelease) { Resolve-ReleaseBinary $FromRelease } else { Resolve-VitalsBinary }
    if ($Version) {
        $reported = (& $vitalsExe --version 2>&1 | Out-String).Trim()
        if ($reported -ne "vitals $Version") {
            throw "-Version $Version was asked for, but $vitalsExe reports '$reported'"
        }
        Write-Host "Vitals binary reports $reported"
    }
    $vitalsPort = Get-FreePort
    $vitalsOutLog = Join-Path (Join-Path $env:TEMP 'vitals-ha-demo') 'vitals.out'
    $vitalsProcess = Start-Process -FilePath $vitalsExe `
        -ArgumentList @('serve', '--source', 'local', '--port', "$vitalsPort", '--mqtt', "mqtt://127.0.0.1:$MqttPort", '--mqtt-interval', '5') `
        -RedirectStandardOutput $vitalsOutLog -RedirectStandardError $vitalsErrLog `
        -PassThru -WindowStyle Hidden
    Write-Host "started $vitalsExe (node $node) against mqtt://127.0.0.1:$MqttPort"

    # Wait for a reading, not just for the entity: an entity Home Assistant
    # created but cannot measure would still be a pass for "accepted" and a
    # false pass for "works".
    $probe = $null
    $probeError = $null
    $attempt = 0
    Write-Host "waiting up to $entityTimeoutSec s for Home Assistant to report a reading from $node"
    $deadline = (Get-Date).AddSeconds($entityTimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $attempt++
        # $null is "Home Assistant did not answer", which is a different fact
        # from "answered and holds nothing" and must not be printed as zero.
        try { $probe = Get-VitalsEntities -Base $base -Token $token -Socket $haSocket -UniqueIdPrefix "${node}_" }
        catch { $probe = $null; $probeError = $_.Exception.Message }
        if ($null -ne $probe -and @($probe.Entities | Where-Object { $_.State -match '^-?[0-9.]+$' }).Count -gt 0) { break }
        # Printed every pass: a demo that goes quiet for a minute looks broken,
        # and the count tells the reader the difference between "Home Assistant
        # has created nothing" and "it created everything and has no reading yet".
        if ($null -eq $probe) { Write-Host "  attempt ${attempt}: no answer ($probeError)" }
        else { Write-Host "  attempt ${attempt}: $(@($probe.Entities).Count) entities, none reporting a reading yet" }
        # A publisher that died on startup (a port it could not bind, a broker
        # address it refused) will never publish, so stop waiting for it — after
        # a probe, so the report still says what Home Assistant holds rather
        # than only that the read failed.
        if ($vitalsProcess.HasExited) { break }
        Start-Sleep -Seconds 2
    }

    Write-Step 'What Home Assistant created'
    if ($null -eq $probe) {
        # The reason is the script's own read failing, and saying so is what
        # keeps a broken probe from being reported as a Home Assistant verdict.
        throw "nothing could be read back from Home Assistant ($probeError)"
    }
    $entities = @($probe.Entities)
    if ($entities.Count -eq 0) {
        Write-Host "Home Assistant created no entity for node ${node}." -ForegroundColor Red
        Write-Host 'What Home Assistant said about the discovery documents:'
        $lines = Get-HaLogLines "mqtt|$node" 20
        if ($lines.Count -eq 0) { Write-Host '  (no matching lines in /config/home-assistant.log)' }
        else { foreach ($line in $lines) { Write-Host "  $line" } }
        $retained = Get-RetainedMessage "$discoveryPrefix/sensor/$node/+/config"
        if ($retained) { Write-Host "The broker is still holding a retained document, which Home Assistant read: $retained" }
        else { Write-Host 'The broker is holding no discovery document at all, so the Vitals side never published.' }
        $all = @(Get-HaWsResult -Socket $haSocket -Type 'config/entity_registry/list')
        $mqttCount = @($all | Where-Object { $_.platform -eq 'mqtt' }).Count
        Write-Host "Home Assistant's entity registry holds $mqttCount MQTT entities in total"
        # `-ExpectRejected` inverts the verdict: for a build that announces in a
        # shape Home Assistant refuses, nothing appearing is the outcome that was
        # asked for, so the run passes. Without it the log lines above are Home
        # Assistant's own reason for the failure and this throws.
        if (-not $ExpectRejected) {
            throw 'the integration created no entity; the log lines above are Home Assistant''s reason'
        }
        Write-Host 'Expected: this publisher announces in a shape Home Assistant refuses, so no entity is the right outcome.' -ForegroundColor Green
        $rejectedAsExpected = $true
        $proofOk = $true
    }
    else {
        if ($ExpectRejected) {
            throw "Home Assistant created $($entities.Count) entities for $node, but -ExpectRejected expected it to refuse the publisher: it no longer fails the way it was believed to"
        }

        Write-Host "Home Assistant now holds $($entities.Count) entities for node ${node}:" -ForegroundColor Green
        $acceptedByHa = $true
        foreach ($entity in ($entities | Sort-Object EntityId)) {
            $state = if ($null -ne $entity.State) { $entity.State } else { '(registered, no state published yet)' }
            Write-Host ("  {0}  {1}   (unique_id {2})" -f $entity.EntityId, $state, $entity.UniqueId)
            $entityReport.Add([ordered]@{
                    entity_id    = $entity.EntityId
                    unique_id    = $entity.UniqueId
                    state        = $entity.State
                    unit         = $entity.Unit
                    device_class = $entity.DeviceClass
                })
        }
        if (@($entities | Where-Object { $_.State -match '^-?[0-9.]+$' }).Count -eq 0) {
            Write-Host 'none of them carries a numeric reading yet; they exist but may still be coming online' -ForegroundColor Yellow
        }

        # The project's headline principle, checked at the consumer's boundary:
        # a reading this machine cannot measure must be absent, and `unknown` is
        # how Home Assistant says absent. A number here would be a fabricated
        # reading that looks like a measurement, and `0` is the number a
        # defaulted-in value would produce.
        if ($AssertAbsent.Count -gt 0) {
            Write-Step 'Readings that cannot be measured are absent, not zero'
            foreach ($key in $AssertAbsent) {
                $uniqueId = "${node}_$key"
                $entity = @($entities | Where-Object { $_.UniqueId -eq $uniqueId })[0]
                if ($null -eq $entity) {
                    throw "no entity carries unique_id $uniqueId, so an absent reading cannot be told from a refused document; check the key, or whether Vitals still announces it"
                }
                if ($null -eq $entity.State -or $entity.State -in @('unknown', 'unavailable')) {
                    Write-Host "  $($entity.EntityId) reads '$($entity.State)' (unit '$($entity.Unit)') — absent, which is the truth for a machine with no such sensor" -ForegroundColor Green
                }
                elseif ($entity.State -match '^-?[0-9.]+$') {
                    throw "$($entity.EntityId) reads $($entity.State), but this machine cannot measure it: a number here is a value nobody measured"
                }
                else {
                    throw "$($entity.EntityId) reads '$($entity.State)', which is neither a reading nor an absent one, so the assertion cannot decide"
                }
            }
        }

        # The entity id is Home Assistant's, so the link is built from what it
        # reported rather than from a guess at its naming.
        $cpu = $entities | Where-Object { $_.UniqueId -eq "${node}_cpu" } | Select-Object -First 1
        if ($null -ne $cpu) { $entityLink = "http://127.0.0.1:$HaPort/history?entity_id=$($cpu.EntityId)" }

        if ($Reproduce) { $reproduceReport = Invoke-Reproduce -Base $base -Token $token -Socket $haSocket -Node $node }

        $proofOk = $true
    }
}
catch {
    $failure = $_
    Show-VitalsStderr
}
finally {
    if ($vitalsProcess -and -not $vitalsProcess.HasExited) {
        # An abrupt stop is deliberate: it drops the connection without a clean
        # DISCONNECT, so the broker publishes the Last Will `offline` exactly as
        # it would if the machine had crashed.
        Stop-Process -Id $vitalsProcess.Id -Force -ErrorAction SilentlyContinue
    }
    # Closing the socket must not be able to skip the teardown below: leaving
    # containers running after a casual run is a worse outcome than a noisy one.
    try {
        if ($haSocket -is [System.Net.WebSockets.ClientWebSocket]) { $haSocket.Dispose() }
    }
    catch { Write-Host "could not close the Home Assistant WebSocket: $($_.Exception.Message)" -ForegroundColor Yellow }

    if ($Keep) {
        Write-Step 'Left running (-Keep)'
        Write-Host "Home Assistant:  http://127.0.0.1:$HaPort"
        Write-Host "  username:      $haUser"
        if ($haPassword) { Write-Host "  password:      $haPassword" } else { Write-Host '  password:      (generated on a previous run; not known here)' }
        Write-Host "Broker:          mqtt://127.0.0.1:$MqttPort  (from the host)"
        Write-Host "Entity:          $entityLink"
        Write-Host "Remove it all:   pwsh -NoProfile -File scripts/ha-demo.ps1 -Prefix $Prefix -Down"
    }
    else {
        Write-Step 'Tearing down'
        # Guarded, so a teardown problem cannot replace the failure that is about
        # to be reported with one of its own: the first error is the useful one.
        try { Remove-DemoResources }
        catch { Write-Host "teardown did not finish: $($_.Exception.Message)" -ForegroundColor Yellow }
    }
}

$expected = if ($ExpectRejected) { 'rejected' } else { 'accepted' }

if (-not $proofOk) {
    Write-Host ''
    Write-Host "FAILED: $($failure.Exception.Message)" -ForegroundColor Red
    if ($Json) {
        Write-JsonResult -Mode 'prove' -Ok $false -Node $node -Accepted $acceptedByHa -Expected $expected `
            -Error $failure.Exception.Message -EntityLink $entityLink -Entities $entityReport -Reproduce $reproduceReport
    }
    exit 1
}

if ($Json) {
    # `accepted` reports what Home Assistant did; `ok` reports whether that was
    # the expected outcome. They are different facts and both are needed: with
    # -ExpectRejected a proved defect is ok=true and accepted=false, while a run
    # that failed a later assertion is ok=false with accepted=true.
    Write-JsonResult -Mode 'prove' -Ok $true -Node $node -Accepted $acceptedByHa -Expected $expected `
        -EntityLink $entityLink -Entities $entityReport -Reproduce $reproduceReport
}

Write-Host ''
if ($rejectedAsExpected) {
    Write-Host 'OK: the publisher under test was refused by Home Assistant, as asked — no entity was created, which is the defect being demonstrated.' -ForegroundColor Green
}
else {
    Write-Host 'OK: Home Assistant accepted the discovery documents and created the entities above.' -ForegroundColor Green
}
exit 0
