$base = "https://github.com/vieetj731/Vietnamese-Inverse-Text-Normalization/raw/master"
$dest = Join-Path $PSScriptRoot "..\resources\itn-vi"
New-Item -ItemType Directory -Force -Path $dest | Out-Null
Invoke-WebRequest "$base/far/classify/tokenize_and_classify.far" -OutFile "$dest\tokenize_and_classify.far"
Invoke-WebRequest "$base/far/verbalize/verbalize.far" -OutFile "$dest\verbalize.far"
Write-Host "ITN FAR files saved to $dest"
