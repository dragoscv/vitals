<#
  A devcloud job is blind while it runs.

  The runner uploads a job's stdout only when a child writes to it, and a lane
  captures its gate's output into a record instead of printing it — so a gate
  that is happily working, and a gate that is wedged, look identical from the
  outside: no new lines, low CPU, no network. On 2026-11-25 that cost an hour of
  inference from CPU graphs; the answer has to come from inside the job.

  Two functions, one file, used by every step of a job so the trail is one
  timeline rather than one file per script:

    Send-DevcloudProgress -Path <file> [-ObjectName progress]   upload one file
    Write-DevcloudStage  '<what is happening>'                  append + upload

  Both are no-ops outside a devcloud job (DEVCLOUD_JOB_ID + DEVCLOUD_BUCKET),
  never throw, and are safe to call from several processes of the same job: they
  append to one file in TEMP and upload it under the job's own prefix, so
  `gcloud storage cat gs://<bucket>/jobs/<id>/stage` is a live transcript.
#>

function Send-DevcloudProgress {
    param([string]$Path, [string]$ObjectName = 'progress', [int]$TimeoutSec = 20)
    if (-not ($env:DEVCLOUD_JOB_ID -and $env:DEVCLOUD_BUCKET)) { return }
    if (-not (Test-Path -LiteralPath $Path)) { return }
    try {
        $meta = 'http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token'
        $token = (Invoke-RestMethod -Uri $meta -Headers @{ 'Metadata-Flavor' = 'Google' } -TimeoutSec 5).access_token
        $uri = 'https://storage.googleapis.com/upload/storage/v1/b/{0}/o?uploadType=media&name=jobs/{1}/{2}' -f $env:DEVCLOUD_BUCKET, $env:DEVCLOUD_JOB_ID, $ObjectName
        Invoke-RestMethod -Method Post -Uri $uri -Headers @{ Authorization = "Bearer $token" } -InFile $Path -TimeoutSec $TimeoutSec | Out-Null
    } catch {
        # Narration is a courtesy to whoever is watching, never a reason to fail a
        # gate: an instance without the metadata route must still be able to build.
    }
}

function Write-DevcloudStage {
    param([string]$Text)
    if (-not ($env:DEVCLOUD_JOB_ID -and $env:DEVCLOUD_BUCKET)) { return }
    try {
        if (-not $script:DevcloudStageFile) {
            # Named after the job: TEMP belongs to the machine, not to the job, so
            # a plain name meant the first lines under `jobs/<id>/stage` could be
            # the tail of the job that ran before this one — measured 2026-11-25,
            # when a job's own trail opened with another job's last three lines.
            $script:DevcloudStageFile = Join-Path ([IO.Path]::GetTempPath()) "devcloud-stage-$env:DEVCLOUD_JOB_ID.txt"
        }
        $line = '{0}Z {1}' -f (Get-Date).ToUniversalTime().ToString('HH:mm:ss'), $Text
        [IO.File]::AppendAllText($script:DevcloudStageFile, $line + "`r`n")
        Send-DevcloudProgress -Path $script:DevcloudStageFile -ObjectName 'stage'
    } catch {
        # Same reasoning as Send-DevcloudProgress.
    }
}
