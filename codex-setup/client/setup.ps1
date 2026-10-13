param([switch]$LibraryOnly)
$ErrorActionPreference = 'Stop'
$script:ToolVersion = '1.0.0'
$script:Utf8 = New-Object System.Text.UTF8Encoding($false, $true)
$script:NettHash = '__NETT_SHA256__'
$script:NettBase64 = '__NETT_BASE64__'
function Initialize-Toml {
    if (!('CprAtomicFile' -as [type])) {
        Add-Type -TypeDefinition 'public static class CprAtomicFile { public static void Replace(string source, string target) { System.IO.File.Replace(source, target, null, true); } }'
    }
    if ('Nett.Toml' -as [type]) { return }
    $bytes = [Convert]::FromBase64String($script:NettBase64)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $hash = ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-','').ToLowerInvariant() } finally { $sha.Dispose() }
    if ($hash -ne $script:NettHash) { throw 'Embedded TOML parser integrity check failed.' }
    [void][Reflection.Assembly]::Load($bytes)
}
function Get-SetupEnvironment {
    $homePath = if ($env:CODEX_HOME) { [IO.Path]::GetFullPath($env:CODEX_HOME) } else { Join-Path $env:USERPROFILE '.codex' }
    if ($homePath.TrimEnd('\','/') -eq [IO.Path]::GetPathRoot($homePath).TrimEnd('\','/')) { throw 'CODEX_HOME cannot be the filesystem root.' }
    $commands = @(Get-Command codex.exe,codex.cmd,codex.ps1 -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source)
    $binRoot = Join-Path $env:LOCALAPPDATA 'OpenAI\Codex\bin'
    if (Test-Path -LiteralPath $binRoot) { $commands += @(Get-ChildItem -LiteralPath $binRoot -Directory -ErrorAction SilentlyContinue | ForEach-Object { $candidate=Join-Path $_.FullName 'codex.exe'; if (Test-Path -LiteralPath $candidate) { $candidate } }) }
    $commands = @($commands | Select-Object -Unique)
    [pscustomobject]@{ User=[Environment]::UserName; Home=$homePath; CodexPaths=$commands; PowerShell=$PSVersionTable.PSVersion.ToString() }
}
function Normalize-CprUrl([string]$Value, [switch]$AllowLoopback) {
    $text=$Value.Trim(); if ($text -notmatch '^[a-zA-Z][a-zA-Z0-9+.-]*://') { $text='https://'+$text }
    $u=$null; if (![Uri]::TryCreate($text,[UriKind]::Absolute,[ref]$u)) { throw 'Invalid CPR URL.' }
    if ($u.UserInfo -or $u.Query -or $u.Fragment) { throw 'URL must not contain credentials, query parameters or fragments.' }
    if ($u.Scheme -ne 'https' -and !($AllowLoopback -and $u.Scheme -eq 'http' -and $u.IsLoopback)) { throw 'HTTPS is required.' }
    $path=$u.AbsolutePath.TrimEnd('/'); if ($path -match '/v1$') { $path=$path.Substring(0,$path.Length-3) }
    if ($path -match '/api/codex/setup$') { $path=$path.Substring(0,$path.Length-16) }
    return $u.GetLeftPart([UriPartial]::Authority)+$path
}
function Get-SetupBundle([string]$RootUrl,[string]$ApiKey,[switch]$AllowLoopback) {
    $root=Normalize-CprUrl $RootUrl -AllowLoopback:$AllowLoopback
    if ($ApiKey -notmatch '^[\x21-\x7e]{8,512}$') { throw 'Invalid API key format.' }
    [void][Reflection.Assembly]::LoadWithPartialName('System.Net.Http')
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    $handler=New-Object Net.Http.HttpClientHandler; $handler.AllowAutoRedirect=$false
    $client=New-Object Net.Http.HttpClient($handler); $client.Timeout=[TimeSpan]::FromSeconds(20)
    $request=New-Object Net.Http.HttpRequestMessage([Net.Http.HttpMethod]::Get,($root+'/api/codex/setup'))
    [void]$request.Headers.TryAddWithoutValidation('Authorization',('Bearer '+$ApiKey))
    [void]$request.Headers.TryAddWithoutValidation('User-Agent',('codex-setup/'+$script:ToolVersion))
    $response=$null; $stream=$null; $memory=$null
    try {
        try { $response=$client.SendAsync($request,[Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult() } catch {
            $failure=$_.Exception;$refused=$false
            while ($failure) { if ($failure -is [Net.Sockets.SocketException] -and $failure.SocketErrorCode -eq [Net.Sockets.SocketError]::ConnectionRefused) {$refused=$true};$failure=$failure.InnerException }
            $proxy=$null;try {$proxy=[Net.WebRequest]::DefaultWebProxy.GetProxy([Uri]($root+'/api/codex/setup'))}catch{}
            if (!$refused -or !$proxy -or !$proxy.IsLoopback -or $proxy.AbsoluteUri -eq ($root+'/api/codex/setup')) {throw 'Connection failed (DNS, TLS, proxy or timeout). No local files were changed.'}
            Write-Host 'Local system proxy refused the connection; retrying direct HTTPS to the same CPR. Proxy settings are unchanged.' -ForegroundColor Yellow
            $request.Dispose();$client.Dispose();$handler.Dispose()
            $handler=New-Object Net.Http.HttpClientHandler;$handler.AllowAutoRedirect=$false;$handler.UseProxy=$false
            $client=New-Object Net.Http.HttpClient($handler);$client.Timeout=[TimeSpan]::FromSeconds(20)
            $request=New-Object Net.Http.HttpRequestMessage([Net.Http.HttpMethod]::Get,($root+'/api/codex/setup'))
            [void]$request.Headers.TryAddWithoutValidation('Authorization',('Bearer '+$ApiKey));[void]$request.Headers.TryAddWithoutValidation('User-Agent',('codex-setup/'+$script:ToolVersion))
            try {$response=$client.SendAsync($request,[Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()}catch{throw 'Direct HTTPS connection also failed. No local files were changed.'}
        }
        $status=[int]$response.StatusCode
        if ($status -ne 200) {
            switch ($status) {
                401 { throw 'API key is invalid or disabled (401).' }
                403 { throw 'API key access denied (403).' }
                404 { throw 'This CPR has no Codex setup endpoint (404).' }
                409 { throw 'No accessible configured default model (409).' }
                default { throw ('Server returned HTTP '+$status+'. No local files were changed.') }
            }
        }
        if (!$response.Content.Headers.ContentType -or $response.Content.Headers.ContentType.MediaType -ne 'application/json') { throw 'Server did not return JSON.' }
        if ($response.Content.Headers.ContentLength -gt 4194304) { throw 'Configuration download is too large.' }
        $stream=$response.Content.ReadAsStreamAsync().GetAwaiter().GetResult();$memory=New-Object IO.MemoryStream
        $buffer=New-Object byte[] 16384;$total=0;$deadline=[DateTime]::UtcNow.AddSeconds(20)
        while ($true) {
            $task=$stream.ReadAsync($buffer,0,$buffer.Length)
            $remaining=[int][Math]::Max(1,($deadline-[DateTime]::UtcNow).TotalMilliseconds)
            if (!$task.Wait($remaining)) { throw 'Configuration download timed out.' }
            $count=$task.GetAwaiter().GetResult();if (!$count) { break };$total+=$count
            if ($total -gt 4194304) { throw 'Configuration download is too large.' };$memory.Write($buffer,0,$count)
        }
        try { $bundle=$script:Utf8.GetString($memory.ToArray()) | ConvertFrom-Json } catch { throw 'Server returned invalid JSON.' }
        Assert-SetupBundle $bundle
        [pscustomobject]@{ RootUrl=$root; Bundle=$bundle }
    } finally {
        if ($stream) {$stream.Dispose()};if ($memory) {$memory.Dispose()};if ($response) {$response.Dispose()};$request.Dispose();$client.Dispose();$handler.Dispose()
    }
}
function Assert-SetupBundle($Bundle) {
    if (!$Bundle -or $Bundle.schema_version -notin @(1,2) -or $Bundle.version -notmatch '^[a-f0-9]{64}$' -or !$Bundle.config -or !$Bundle.models_json) { throw 'Unsupported configuration schema.' }
    $models=@($Bundle.models_json.models)
    if ($models.Count -eq 0 -or $models.Count -gt 2000) { throw 'Invalid model catalog.' }
    $seen=New-Object 'Collections.Generic.HashSet[string]' ([StringComparer]::Ordinal)
    foreach ($m in $models) {
        if ($m.slug -isnot [string] -or $m.slug -notmatch '^\S{1,256}$' -or !$seen.Add($m.slug) -or $m.display_name -isnot [string] -or !$m.display_name) { throw 'Invalid or duplicate model.' }
        foreach ($field in @('context_window','max_context_window')) {
            $n=$m.$field;if ($n -isnot [int] -and $n -isnot [long]) { throw 'Invalid model context limit.' };if ($n -le 0 -or $n -gt 9007199254740991) { throw 'Invalid model context limit.' }
        }
        if ($m.shell_type -ne 'unified_exec' -or $m.visibility -ne 'list' -or $m.supported_in_api -ne $true -or $m.base_instructions -isnot [string]) { throw 'Invalid Codex model metadata.' }
        foreach ($level in @($m.supported_reasoning_levels)) { if ($level.effort -notin @('none','minimal','low','medium','high','xhigh','max','ultra') -or $level.description -isnot [string]) { throw 'Invalid reasoning metadata.' } }
        if (@($m.input_modalities).Count -eq 0) { throw 'Missing input modalities.' }
        foreach ($modality in @($m.input_modalities)) { if ($modality -notin @('text','image','audio')) { throw 'Unsupported input modality.' } }
    }
    $c=$Bundle.config
    $allowed=@('model','review_model','model_reasoning_effort','service_tier','supports_websockets','remote_compaction','managed_actor_header','image_generation','goals','stream_idle_timeout_ms')
    foreach ($prop in $c.PSObject.Properties) { if ($prop.Name -notin $allowed) { throw 'Unexpected client configuration field.' } }
    if (!$seen.Contains([string]$c.model) -or !$seen.Contains([string]$c.review_model) -or $c.service_tier -ne 'default' -or $c.remote_compaction -notin @('v2','unsupported')) { throw 'Invalid client defaults.' }
    foreach ($field in @('supports_websockets','managed_actor_header','image_generation','goals')) { if ($c.$field -isnot [bool]) { throw 'Invalid feature flag.' } }
    if ($Bundle.schema_version -eq 2 -or $c.PSObject.Properties.Name -contains 'stream_idle_timeout_ms') {
        $n=$c.stream_idle_timeout_ms
        if (($n -isnot [int] -and $n -isnot [long]) -or $n -lt 1000 -or $n -gt 3600000) {throw 'Invalid stream idle timeout.'}
    }
    $model=@($models | Where-Object {$_.slug -ceq $c.model})[0]
    if ($c.model_reasoning_effort -and $c.model_reasoning_effort -notin @($model.supported_reasoning_levels | ForEach-Object {$_.effort})) { throw 'Default reasoning effort is not supported by the model.' }
}
function Quote-Toml([string]$Text) {
    # JSON-compatible escaping restricted to sequences TOML basic strings support.
    $sb=New-Object Text.StringBuilder;[void]$sb.Append('"')
    foreach ($ch in $Text.ToCharArray()) {
        switch ([int]$ch) { 34 {[void]$sb.Append('\"')} 92 {[void]$sb.Append('\\')} 8 {[void]$sb.Append('\b')} 9 {[void]$sb.Append('\t')} 10 {[void]$sb.Append('\n')} 12 {[void]$sb.Append('\f')} 13 {[void]$sb.Append('\r')} default {if ([int]$ch -lt 32 -or [int]$ch -eq 127) {[void]$sb.Append(('\u{0:x4}' -f [int]$ch))} else {[void]$sb.Append($ch)}} }
    };[void]$sb.Append('"');$sb.ToString()
}
function Merge-CodexConfig([string]$Existing,$Bundle,[string]$RootUrl,[string]$ApiKey,[string]$ModelsPath) {
    Initialize-Toml;Assert-SetupBundle $Bundle
    if ($ApiKey -notmatch '^[\x21-\x7e]{8,512}$') { throw 'Invalid API key format.' }
    try { $old=[Nett.Toml]::ReadString($Existing) } catch { throw 'Existing config.toml cannot be parsed safely. It was not modified.' }
    $c=$Bundle.config;$lines=New-Object 'Collections.Generic.List[string]'
    $lines.Add('model_provider = "Codex Proxy RS"');$lines.Add('model = '+(Quote-Toml $c.model));$lines.Add('review_model = '+(Quote-Toml $c.review_model))
    if ($c.model_reasoning_effort) {$lines.Add('model_reasoning_effort = '+(Quote-Toml $c.model_reasoning_effort))}
    $lines.Add('service_tier = "default"');$lines.Add('model_catalog_json = '+(Quote-Toml ($ModelsPath.Replace('\','/'))))
    $lines.Add('[model_providers."Codex Proxy RS"]');$lines.Add('name = "Codex Proxy RS"');$lines.Add('base_url = '+(Quote-Toml ($RootUrl+'/v1')));$lines.Add('wire_api = "responses"')
    $idle=if ($c.PSObject.Properties.Name -contains 'stream_idle_timeout_ms') {$c.stream_idle_timeout_ms} else {900000}
    $lines.Add('stream_idle_timeout_ms = '+$idle)
    $lines.Add('supports_websockets = '+$c.supports_websockets.ToString().ToLowerInvariant());$lines.Add('requires_openai_auth = false');$lines.Add('experimental_bearer_token = '+(Quote-Toml $ApiKey))
    $lines.Add('[model_providers."Codex Proxy RS".capabilities]');$lines.Add('remote_compaction = '+(Quote-Toml $c.remote_compaction))
    if ($c.managed_actor_header) {$lines.Add('[model_providers."Codex Proxy RS".http_headers]');$lines.Add('X-OpenAI-Actor-Authorization = "proxy-managed"')}
    $lines.Add('[features]');$lines.Add('image_generation = '+$c.image_generation.ToString().ToLowerInvariant());$lines.Add('goals = '+$c.goals.ToString().ToLowerInvariant())
    $managed=[Nett.Toml]::ReadString(($lines -join "`n"))
    foreach ($key in @('model_provider','model','review_model','model_reasoning_effort','service_tier','model_catalog_json')) {
        if ($managed.ContainsKey($key)) {$old[$key]=$managed[$key]} elseif ($key -eq 'model_reasoning_effort') {[void]$old.Remove($key)}
    }
    foreach ($section in @('model_providers','features')) {
        if (!$old.ContainsKey($section)) {$old[$section]=[Nett.Toml]::Create()}
        if ($old[$section] -isnot [Nett.TomlTable]) {throw ('Existing '+$section+' is not a table; cannot merge safely.')}
        if ($section -eq 'model_providers') {
            # Migrate only an old tool-owned CPR block for the same endpoint and key.
            if ($old[$section].ContainsKey('CPR') -and $old[$section]['CPR'] -is [Nett.TomlTable]) {
                $legacy=$old[$section]['CPR']
                if ($legacy.ContainsKey('name') -and $legacy.ContainsKey('base_url') -and $legacy.ContainsKey('experimental_bearer_token') -and $legacy['name'].Get([string]) -ceq 'CPR' -and $legacy['base_url'].Get([string]) -ceq ($RootUrl+'/v1') -and $legacy['experimental_bearer_token'].Get([string]) -ceq $ApiKey) {[void]$old[$section].Remove('CPR')}
            }
            $old[$section]['Codex Proxy RS']=$managed[$section]['Codex Proxy RS']
        } else {foreach ($key in @('image_generation','goals')) {$old[$section][$key]=$managed[$section][$key]}}
    }
    try { $text=[Nett.Toml]::WriteString($old);[void][Nett.Toml]::ReadString($text) } catch {throw 'Merged TOML validation failed.'}
    return $text
}
function Quote-JsonText([string]$Text) {
    $sb=New-Object Text.StringBuilder;[void]$sb.Append('"')
    for ($i=0;$i -lt $Text.Length;$i++) {
        $ch=$Text[$i];$n=[int]$ch
        switch ($n) {34 {[void]$sb.Append('\"')} 92 {[void]$sb.Append('\\')} 8 {[void]$sb.Append('\b')} 9 {[void]$sb.Append('\t')} 10 {[void]$sb.Append('\n')} 12 {[void]$sb.Append('\f')} 13 {[void]$sb.Append('\r')} default {
            $unpaired=([char]::IsHighSurrogate($ch) -and ($i+1 -eq $Text.Length -or ![char]::IsLowSurrogate($Text[$i+1]))) -or ([char]::IsLowSurrogate($ch) -and ($i -eq 0 -or ![char]::IsHighSurrogate($Text[$i-1])))
            if ($n -lt 32 -or $unpaired) {[void]$sb.Append(('\u{0:x4}' -f $n))} else {[void]$sb.Append($ch)}
        }}
    };[void]$sb.Append('"');$sb.ToString()
}
function ConvertTo-InlineJson($Value) {
    if ($null -eq $Value) {return 'null'}
    if ($Value -is [string]) {return Quote-JsonText $Value}
    if ($Value -is [bool]) {return $Value.ToString().ToLowerInvariant()}
    if ($Value -is [Collections.IDictionary]) {
        $pairs=@(foreach($key in $Value.Keys) {(Quote-JsonText ([string]$key))+': '+(ConvertTo-InlineJson $Value[$key])})
        return '{'+($pairs -join ', ')+'}'
    }
    if ($Value -is [Collections.IEnumerable]) {
        $items=@(foreach($item in $Value) {ConvertTo-InlineJson $item});return '['+($items -join ', ')+']'
    }
    if ($Value -is [IFormattable]) {return $Value.ToString($null,[Globalization.CultureInfo]::InvariantCulture)}
    throw 'Unsupported JSON value in model catalog.'
}
function Format-CodexCatalog($Catalog) {
    $blocks=New-Object 'Collections.Generic.List[string]'
    foreach($m in @($Catalog.models)) {
        $levels=@(foreach($level in @($m.supported_reasoning_levels)) {[ordered]@{effort=$level.effort;description=$level.description}})
        # Codex-only compatibility choice; this does not alter the basic registry.
        if ('ultra' -notin @($levels | ForEach-Object {$_.effort})) {$levels+=,[ordered]@{effort='ultra';description='Ultra reasoning effort'}}
        $fields=[ordered]@{
            slug=$m.slug;display_name=$m.display_name;supported_reasoning_levels=$levels;shell_type=$m.shell_type;visibility=$m.visibility;supported_in_api=$m.supported_in_api;priority=$m.priority;base_instructions=$m.base_instructions;support_verbosity=$m.support_verbosity;truncation_policy=[ordered]@{mode=$m.truncation_policy.mode;limit=$m.truncation_policy.limit};experimental_supported_tools=[object[]]@($m.experimental_supported_tools);context_window=$m.context_window;max_context_window=$m.max_context_window;input_modalities=[object[]]@($m.input_modalities);supports_reasoning_summary_parameter=$m.supports_reasoning_summary_parameter
        }
        $lines=New-Object 'Collections.Generic.List[string]'
        foreach($key in $fields.Keys) {
            $value=$fields[$key]
            if ($key -eq 'supported_reasoning_levels' -and @($value).Count) {
                $items=@(foreach($level in $value) {'        '+(ConvertTo-InlineJson $level)})
                $text="[`n"+($items -join ",`n")+"`n      ]"
            } else {$text=ConvertTo-InlineJson $value}
            $lines.Add('      '+(Quote-JsonText $key)+': '+$text)
        }
        $blocks.Add("    {`n"+($lines -join ",`n")+"`n    }")
    }
    if (!$blocks.Count) {return "{`n  `"models`": []`n}`n"}
    return "{`n  `"models`": [`n"+($blocks -join ",`n")+"`n  ]`n}`n"
}
function Get-FileState([string]$Path) {
    if (!(Test-Path -LiteralPath $Path)) { return '<absent>' }
    if ((Get-Item -LiteralPath $Path).PSIsContainer) { throw 'A target filename is a directory.' }
    (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
}
function Assert-SafeTarget([string]$Path) {
    $p=[IO.Path]::GetFullPath($Path)
    while ($p) {
        if (Test-Path -LiteralPath $p) { $item=Get-Item -LiteralPath $p -Force;if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {throw 'Symlink/junction configuration targets are not supported.'} }
        $parent=[IO.Path]::GetDirectoryName($p);if ($parent -eq $p) {break};$p=$parent
    }
}
function Move-Atomic([string]$Temp,[string]$Target) {
    if ([IO.File]::Exists($Target)) { [CprAtomicFile]::Replace($Temp,$Target) } else { [IO.File]::Move($Temp,$Target) }
}
function Install-CodexSetup($Bundle,[string]$RootUrl,[string]$ApiKey,[string]$ConfigHome) {
    Assert-SetupBundle $Bundle
    $root=[IO.Path]::GetFullPath($ConfigHome);if ($root.TrimEnd('\') -eq [IO.Path]::GetPathRoot($root).TrimEnd('\')) {throw 'Configuration directory cannot be the filesystem root.'}
    Assert-SafeTarget $root
    $config=Join-Path $root 'config.toml';$models=Join-Path $root 'models.json';Assert-SafeTarget $config;Assert-SafeTarget $models
    $oldConfigHash=Get-FileState $config;$oldModelsHash=Get-FileState $models
    $existing=if ($oldConfigHash -ne '<absent>') {[IO.File]::ReadAllText($config,$script:Utf8)} else {''}
    $newConfig=Merge-CodexConfig $existing $Bundle $RootUrl $ApiKey $models
    $newModels=Format-CodexCatalog $Bundle.models_json
    [void]($newModels | ConvertFrom-Json)
    if (!(Test-Path -LiteralPath $root)) {[void][IO.Directory]::CreateDirectory($root)}
    $lock=$null;$tmpConfig=$null;$tmpModels=$null;$backup=$null;$written=New-Object 'Collections.Generic.List[string]'
    try {
        $lock=[IO.File]::Open((Join-Path $root '.cpr-setup.lock'),[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
        if ((Get-FileState $config) -ne $oldConfigHash -or (Get-FileState $models) -ne $oldModelsHash) {throw 'Files changed while preparing configuration. Retry after closing the other editor.'}
        $id=[Guid]::NewGuid().ToString('N');$tmpConfig=Join-Path $root ('.config-'+$id+'.tmp');$tmpModels=Join-Path $root ('.models-'+$id+'.tmp')
        [IO.File]::WriteAllText($tmpConfig,$newConfig,$script:Utf8);[IO.File]::WriteAllText($tmpModels,$newModels,$script:Utf8)
        $backupRoot=Join-Path $root 'cpr-setup-backups';Assert-SafeTarget $backupRoot
        $backup=Join-Path $backupRoot ((Get-Date -Format 'yyyyMMdd-HHmmss')+'-'+$id.Substring(0,8));[void][IO.Directory]::CreateDirectory($backup)
        if ($oldConfigHash -ne '<absent>') {[IO.File]::Copy($config,(Join-Path $backup 'config.toml'))}
        if ($oldModelsHash -ne '<absent>') {[IO.File]::Copy($models,(Join-Path $backup 'models.json'))}
        if ((Get-FileState $config) -ne $oldConfigHash -or (Get-FileState $models) -ne $oldModelsHash) {throw 'Files changed before writing. No configuration was replaced.'}
        Move-Atomic $tmpModels $models;$written.Add($models)
        Move-Atomic $tmpConfig $config;$written.Add($config)
        [void][Nett.Toml]::ReadString([IO.File]::ReadAllText($config,$script:Utf8));[void]([IO.File]::ReadAllText($models,$script:Utf8) | ConvertFrom-Json)
        [pscustomobject]@{ ConfigPath=$config;ModelsPath=$models;BackupPath=$backup;ModelCount=@($Bundle.models_json.models).Count;Version=$Bundle.version }
    } catch {
        $rollbackFailed=$false
        foreach ($target in $written) {
            try {
                $source=Join-Path $backup ([IO.Path]::GetFileName($target))
                if ([IO.File]::Exists($source)) {$restore=Join-Path $root ('.restore-'+[Guid]::NewGuid().ToString('N')+'.tmp');[IO.File]::Copy($source,$restore);Move-Atomic $restore $target}
                else {if ([IO.File]::Exists($target)) {[IO.File]::Delete($target)}}
            } catch {$rollbackFailed=$true}
        }
        if ($rollbackFailed) {throw ('Write failed and automatic rollback was incomplete. Restore files from '+$backup)}
        throw
    } finally {
        foreach ($temp in @($tmpConfig,$tmpModels)) {if ($temp -and [IO.File]::Exists($temp)) {[IO.File]::Delete($temp)}}
        if ($lock) {$lock.Dispose()}
    }
}
function Read-ConsoleInput([string]$Prompt,[switch]$Secret) {
    Write-Host $Prompt -NoNewline;$s=New-Object Text.StringBuilder
    while ($true) {
        $k=[Console]::ReadKey($true)
        if ($k.Key -eq [ConsoleKey]::Escape) {Write-Host '';return $null}
        if ($k.Key -eq [ConsoleKey]::Enter) {Write-Host '';return $s.ToString()}
        if ($k.Key -eq [ConsoleKey]::Backspace) {if ($s.Length -gt 0) {[void]$s.Remove($s.Length-1,1);Write-Host "`b `b" -NoNewline};continue}
        if (!$k.KeyChar -or [char]::IsControl($k.KeyChar)) {continue};[void]$s.Append($k.KeyChar)
        if ($Secret) {Write-Host '*' -NoNewline} else {Write-Host $k.KeyChar -NoNewline}
    }
}
function Start-Setup {
    [Console]::OutputEncoding=New-Object Text.UTF8Encoding($false)
    Write-Host ('CPR Codex Setup v'+$script:ToolVersion) -ForegroundColor Cyan
    Write-Host 'No administrator rights required. Press Esc to cancel.'
    $envInfo=Get-SetupEnvironment
    Write-Host ('Windows user: '+$envInfo.User);Write-Host ('Codex config: '+$envInfo.Home)
    if ($envInfo.CodexPaths.Count) {foreach ($p in $envInfo.CodexPaths) {Write-Host ('Codex detected: '+$p)}} else {Write-Host 'Codex was not found in known locations. This tool configures it but does not install it.' -ForegroundColor Yellow}
    while ($true) {
        $url=Read-ConsoleInput 'CPR URL: ';if ($null -eq $url) {return}
        $key=Read-ConsoleInput 'API Key: ' -Secret;if ($null -eq $key) {return}
        try {$download=Get-SetupBundle $url $key;break} catch {Write-Host $_.Exception.Message -ForegroundColor Red;$key=$null;Write-Host 'Try again, or press Esc.'}
    }
    try {
        while ($true) {
            Write-Host '';Write-Host ('Connected: '+$download.RootUrl) -ForegroundColor Green
            Write-Host '1. Configure Codex';Write-Host 'Esc. Exit'
            $choice=[Console]::ReadKey($true);if ($choice.Key -eq [ConsoleKey]::Escape) {return};if ($choice.KeyChar -ne '1') {continue}
            # Refresh at selection time: avoid installing a stale snapshot from the initial connection check.
            try {
                $download=Get-SetupBundle $download.RootUrl $key
                Write-Host ('Target: '+$envInfo.Home);Write-Host ('Models: '+@($download.Bundle.models_json.models).Count+'; default: '+$download.Bundle.config.model)
                Write-Host 'Close Codex before applying. models.json will be replaced; other providers/settings will be kept.'
                Write-Host 'The Codex Proxy RS provider block is managed by this tool and will be replaced.'
                Write-Host 'Existing files will be backed up. config.toml stores your API key.'
                Write-Host 'Press Enter to apply, or Esc to cancel.'
                $confirm=[Console]::ReadKey($true);if ($confirm.Key -ne [ConsoleKey]::Enter) {continue}
                $result=Install-CodexSetup $download.Bundle $download.RootUrl $key $envInfo.Home
                Write-Host 'Configuration completed.' -ForegroundColor Green;Write-Host ('Backup: '+$result.BackupPath)
                Write-Host 'Fully quit and reopen Codex to load the configuration.'
                Write-Host 'Press any key to exit.'
                [void][Console]::ReadKey($true)
                return
            } catch {Write-Host $_.Exception.Message -ForegroundColor Red}
        }
    } finally {$key=$null}
}
if (!$LibraryOnly) {try {Start-Setup} catch {Write-Host $_.Exception.Message -ForegroundColor Red;exit 1}}
